# Reliability Design — providers (U4)

Consumes: `nfr-requirements/reliability-requirements.md` (NFR4.1–4.13),
`functional-design/functional-spec.md` (WF2–WF3).
Questions: `nfr-design-questions.md` Q4, Q5.

## The rebuild critical section (NFR4.1–4.3)

```rust
// crates/oxidean-api-oxpilot/src/settings.rs  (api-oxpilot owns ALL of
// this — api-core holds only the inert OxpilotRuntime/OxpilotRuntimeSlot
// types; the api-core → api-oxpilot dep direction does not exist)
async fn settings_set(ctx, payload) -> Result<SettingsView> {
    let runtime = ctx.oxpilot.as_ref().ok_or_else(|| rpc("internal"))?;
    let _g = runtime.rebuild_lock.lock().await;              // ONE mutex
    let candidate = apply(&stored_row(ctx).await?, &payload);        // BR2.3/BR3.4 semantics
    let resolved = resolve_over_env(&candidate);
    validate(&resolved, &payload).await?;                    // async: DNS
    let prior = runtime.slot.read().await.clone_slot();      // Arc clones only
    let slot = if resolved.enabled
        && touches_provider_fields(&payload)
        && resolved.provider_id.is_some() {
        build_slot(&resolved, &ctx.db, &prior.turns).await
            .map_err(|_| rpc("invalid_provider"))?;          // BEFORE persist
    } else {
        resolve_slot(&resolved, &prior)                      // no-build path
    };
    upsert_row(&ctx.db, &candidate).await?;
    *runtime.slot.write().await = slot;
    if let Err(e) =
        audit_oxpilot_settings(ctx, "settings_set", &payload.field_names()).await
    {
        warn!("oxpilot settings audit lost: {e}");           // fire-and-log —
    }                                                       // never fail post-commit
    Ok(view(&resolved))
}

// resolve_slot — the build-skipping path. `prior` supplies the Arc it
// may carry, gated by build-origin identity:
fn resolve_slot(resolved: &EffectiveProviderConfig, prior: &OxpilotRuntimeSlot)
    -> OxpilotRuntimeSlot
{
    // `built_from` is Some only on slots whose provider actually built —
    // the slot records the resolved tuple it was built from, so "same
    // provider config" is a data check, not a payload-side guess:
    // ProviderIdentity { provider_id, model, base_url, key_source(Env|Db|None) }
    let identity = resolved.provider_identity();      // Option<ProviderIdentity>
    let carry = prior.provider.clone()
        .filter(|_| prior.built_from.as_ref() == identity.as_ref());

    if !resolved.enabled {
        // flag-off: keep the Arc ONLY while the row still describes the
        // provider it was built from — a flag-off write that changes
        // provider identity drops the stale Arc (never built, never
        // rejects: recovery stays open, NFR4.3)
        return OxpilotRuntimeSlot {
            provider: carry.clone(),
            model: resolved.model.clone(),
            disabled_reason: Some("flag_off".into()),
            turns: prior.turns.clone(),
            built_from: carry.and(prior.built_from.clone()),
        };
    }
    match (resolved.provider_id, carry) {
        (None, _) => disabled_slot(prior.turns.clone(), "no_provider"),
        (Some(_), Some(_)) =>
            // unchanged flag/budget write: carry the installed provider
            OxpilotRuntimeSlot { provider: prior.provider.clone(),
                                 model: resolved.model.clone(),
                                 disabled_reason: None,
                                 turns: prior.turns.clone(),
                                 built_from: prior.built_from.clone() },
        (Some(_), None) =>
            // enabled, provider resolved, but nothing carryable —
            // prior was misconfigured/dropped/never built. An enabled
            // slot with no provider MUST still carry a reason; the
            // provider returns via a provider-field write (build arm)
            // or the next rebuild
            disabled_slot(prior.turns.clone(), "misconfigured"),
    }
}
```

- Build BEFORE persist ⇒ failed builds persist nothing (NFR4.1).
- One mutex wraps the whole section ⇒ row/slot/audit never diverge
  (NFR4.2); flag+budget-only writes skip the build (NFR4.3) but keep
  the installed provider Arc via `prior` — a budget write can never
  silently uninstall a healthy provider.
- `built_from` is the carry test: `build_slot` stamps
  `Some(resolved.provider_identity())` on success; `disabled_slot` and
  dropped-provider slots leave it `None`. The write's payload set picks
  the arm (any {provider_id, model, base_url, api_key, clear_key,
  clear_overrides} field on an enabled write ⇒ build arm); `built_from`
  picks whether the prior Arc survives within the no-build arm. Env
  drift between builds is accepted staleness.
- Both incoherent shapes are now unreachable: an enabled slot with
  `provider: None` always carries `disabled_reason` (the
  `(Some(_), None)` arm), and a flag-on write can't serve a stale Arc
  because `built_from` mismatch drops it to `misconfigured`.
- Audit failure after commit is `warn!` only — response success must
  not contradict committed state; the crash-gap bound stands (NFR4.2).
- Provider-less resolved configs install `provider: None`+reason slots —
  `clear_overrides` and fresh deployments are writable (NFR4.4).

## In-flight survival (NFR4.5–4.6)

`build_slot` clones the caller's `Arc<ActiveTurns>` into the new slot —
in-flight turns keep their captured provider Arc and map entries; the
swap is one write-lock assignment. Disabled states are reason-bearing
slots, so `oxpilot.status` and the gate always answer.

## Provider-call bounds (NFR4.7–4.10)

```rust
// per-adapter client construction
reqwest::Client::builder()
    .connect_timeout(Duration::from_secs(10))
    .timeout(Duration::from_secs(300))
    .redirect(redirect::Policy::none())
    .build()
// retry: err.is_connect() → at most ONE retry; otherwise never.
// sent-but-unanswered ⇒ classified ProviderError, no retry (no double-bill)
```

Connect-phase classification rides reqwest's `is_connect()` error kind;
stream stalls and HTTP failures map straight into the per-adapter
`ProviderError.code` table → C2 terminal frame.

## Boot & degradation (NFR4.11–4.12)

`AppState::new` (main.rs:116) is sync and never migrates —
`OxpilotRuntime::new` installs the env-only slot (U1 parity).
`oxidean-api/src/main.rs` calls
`oxidean_api_oxpilot::settings::rebuild(&state)` once, right after
`AppState::new` returns — UNCONDITIONALLY, not inside the
`OXIDEAN_AUTO_MIGRATE` branch (main.rs:73 is where migrate runs, gated;
the rebuild must run regardless or an env-configured provider would sit
`misconfigured` forever on a no-migrate boot). `rebuild` takes the
rebuild mutex, reads the settings row, resolves row-over-env, builds,
and swaps — sharing `resolve_over_env`/`build_slot`/`resolve_slot` with
`settings_set`'s critical section. With `AUTO_MIGRATE=false` and the
table absent, the row-read-failure arm below degrades to env-only —
the same posture U1 shipped.

Two degraded mappings are pinned (NFR4.12):
- Row read failure during `rebuild` ⇒ `warn!` + env-only resolution —
  never panics, never blocks listen.
- Resolved-provider build failure on the boot/`rebuild` path ⇒ slot =
  `provider: None` + `disabled_reason: "misconfigured"` (degraded
  install, not a failed boot). This differs from `settings_set`, where
  the same build failure REJECTS the write with `invalid_provider` and
  leaves the prior slot untouched — the write surface fails loudly,
  the boot surface degrades quietly.
- Unknown `provider_id` values (row or env) ⇒ `misconfigured` on the
  same degraded path.

## Custody integrity (NFR4.13)

Row writes carry `key_ciphertext`+`key_last4` as one update; `clear_key`/
`clear_overrides` NULL both in the same statement — no half-written row.
