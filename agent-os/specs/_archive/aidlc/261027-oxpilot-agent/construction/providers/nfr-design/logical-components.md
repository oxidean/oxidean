# Logical Components — providers (U4)

Consumes: the five NFR-design docs + `functional-design/entities.md`.
Component decomposition for code generation — every component is code
we write, not infra we deploy.

## Component catalogue

```yaml
components:
  - name: oxpilot.providers
    crate: oxidean-api-oxpilot
    path: src/providers/{registry,openai,anthropic,ollama,stream,key,egress}.rs
    owns:
      - REGISTRY static table (3 public descriptors for providers_list) +
        RESOLUTION_MAP (4 members — adds the env-only `stub` entry,
        which settings_set refuses with invalid_provider outside test
        builds; env `OXIDEAN_OXPILOT_PROVIDER=stub` stays legal in every
        build, matching U1 build_oxpilot_slot + CONFIGURATION.md)
      - per-adapter impls of U1's LlmProvider (reqwest SSE/NDJSON)
      - shared SseDecoder + NdjsonDecoder → ChatStreamItem
      - ProviderError.code classification tables
      - oxpilot_key custody helpers — encrypt_key/last4 visible to
        settings; decrypt_key is `pub(in crate::providers)` so only
        construction code can open the key (settings procs are same-crate
        so pub(crate) would NOT enforce the boundary)
      - egress::validate_base_url(provider_id: Option<&str>, url) —
        None ⇒ scheme/shape check only (no consumer exists)
      - providers::build(cfg) -> Result<Arc<dyn LlmProvider>, ProviderError>
    reason: single unit boundary — adapters are the unit's purpose

  - name: oxpilot.settings
    crate: oxidean-api-oxpilot
    path: src/settings.rs + dispatch.rs + procedures.rs
    owns:
      - admin.oxpilot.settings_get / settings_set / providers_list procs
      - EffectiveProviderConfig resolution (row-over-env merge)
      - settings_set critical section (mutex → resolve → validate → build → persist → swap → audit)
      - rebuild(state) — the boot entry main.rs calls once after
        AppState::new; shares resolve_over_env/build_slot/resolve_slot
        with settings_set's critical section
      - resolve_slot(&resolved, &prior) — no-build slot computation
      - audit_oxpilot_settings helper
      - closed PROCEDURES-set registration + pinning test
    reason: the unit's admin write surface; resolution lives here (not
      api-core) because api-oxpilot → api-core is a one-way dep

  - name: oxidean_db.oxpilot_settings
    crate: oxidean-db
    path: src/oxpilot_settings.rs + migrations/{sqlite,postgres,mysql}/00XX_oxpilot_settings.sql
    owns:
      - singleton-row read/upsert/clear (all dialects)
      - key pair atomicity (ciphertext+last4 in one statement)
    reason: dialect-specific logic lives only in oxidean-db (project rule)

  - name: api_core.oxpilot_runtime
    crate: oxidean-api-core
    path: src/oxpilot.rs (extend)
    owns:
      - OxpilotRuntime { slot: RwLock<OxpilotRuntimeSlot>, rebuild_lock: tokio::sync::Mutex<()> }
      - OxpilotRuntimeSlot { provider: Option<Arc<dyn LlmProvider>>,
        model: Option<String>, disabled_reason: Option<String>,
        turns: Arc<ActiveTurns>,
        built_from: Option<ProviderIdentity> } — ProviderIdentity
        { provider_id, model, base_url, key_source(Env|Db|None) }
        records the resolved tuple a built provider came from; it is
        Some iff provider actually built (the carry test — see
        reliability-design). turns lives ONLY on the slot — always the
        same shared Arc; nothing duplicates it
      - resolve_oxpilot / require_enabled gate reads (read-lock, no DB)
      - OxpilotRuntime::new(env_slot) — sync env-only boot construction
    reason: the seam U1 installed; the holder embeds in RpcCtx/AppState
      so the types stay in api-core — but construction/rebuild logic
      must live in api-oxpilot (one-way dep)

  - name: api_core.oxpilot (existing)
    crate: oxidean-api-core
    owns: LlmProvider trait (C4 shape), ChatRequest/ChatStreamItem/
      ToolCall/ToolSpec/Done{usage}, StubProvider
    delta: trait gains tools/ToolCall/Done{usage}; ToolSpec and Usage are
      new payload types declared beside ChatRequest — U1's stub adapts
      (FD Q7)
```

## Dependency shape

```text
oxidean-db.oxpilot_settings ─┐
api_core.oxpilot (trait) ────┼─> oxpilot.providers (build+adapters)
                             │           ▲
                             │           │ builds slot values for
api_core.oxpilot_runtime ────┴─> oxpilot.settings (procs + rebuild)
   (types + gate reads only)       │
                                   └─> session + admin gates (RpcCtx helpers)
shell (oxidean-api main.rs) ── calls ──> oxpilot.settings::rebuild(state)
```

`oxpilot.settings` is the only component calling `providers::build` —
custody and construction stay inside the serialized section.
`decrypt_key` is `pub(in crate::providers)`: the settings module passes
`key_ciphertext` into `providers::build`, which owns decryption inside
provider-construction memory. No decrypted material crosses the
crate-internal boundary.

## Ownership notes

- `OxpilotRuntime`/`OxpilotRuntimeSlot` live in api-core because
  `RpcCtx`/`AppState` embed the holder — same crate as U1's slot. They
  are inert types: all construction, resolution, and rebuild logic lives
  in `oxpilot.settings` since the api-core → api-oxpilot direction does
  not exist.
- `RpcCtx.oxpilot` keeps its U1 `Option` shape: `Option<Arc<OxpilotRuntime>>`.
  `None` ⇒ every slot read is `provider: None` + `no_provider` (tests
  and disabled deployments).
- The rebuild mutex + RwLock are `tokio::sync` primitives (the write
  lock is held across an `.await` — the slot swap — so std::sync::RwLock
  is not usable here). reqwest + async-trait are new Cargo deps on
  api-oxpilot; wiremock is a dev-dep only (NFR8.1) backing the per-
  adapter golden fixtures — `fixtures/{openai,anthropic,ollama}/*.sse`
  and `*.ndjson` streams plus the 401/403/429/context-length/5xx
  response set.
- Boot call site: `AppState::new` (main.rs:116) builds
  `OxpilotRuntime::new` with an env-only slot synchronously;
  `main.rs` then calls `oxpilot::settings::rebuild(&state)` ONCE,
  unconditionally — outside the `OXIDEAN_AUTO_MIGRATE` branch (migrate
  is main.rs:73). With auto-migrate off and the table absent the
  rebuild's row-read failure degrades to env-only — the same posture
  U1 shipped — instead of pinning an env-configured provider at
  `misconfigured` forever.
- `admin.oxpilot.*` stays inside the crate's closed PROCEDURES set —
  NFR8.2's collect-only coverage rides U1's pinning-test pattern.

## U1 call-site blast radius (BR5.5 / NFR4.1)

The `SharedOxpilot` → `OxpilotRuntime` upgrade touches exactly:
- `RpcCtx.oxpilot` field type (`oxidean-api-core/src/rpc.rs:134`; ctx
  build sites in api-core `app.rs` handler wrap).
- `AppState.oxpilot` field (`oxidean-api-core/src/app.rs:94`) +
  `AppState::new`/`with_oxpilot` test hook — `SharedOxpilot` becomes
  `Arc<OxpilotRuntime>` (non-Option on AppState; the RpcCtx Option
  shape stays for proc-level flexibility).
- `crates/oxidean-api-oxpilot/src/routes.rs` — `require_enabled` /
  `resolve_oxpilot` reads become `.slot.read().await`.
- `crates/oxidean-api-oxpilot/src/status.rs` — status reads the slot;
  the `misconfigured` reason is now a real reachable variant.
- `crates/oxidean-api-oxpilot/src/session.rs` turn start — reads
  `slot.provider`/`slot.model`/`slot.turns` instead of `OxpilotSlot`
  fields.
- `tests/oxpilot.rs` — slot construction switches from
  `build_oxpilot_slot(env)` to `OxpilotRuntime::new` + `rebuild`.
- `main.rs` — one unconditional `rebuild(&state)` call after
  `AppState::new` (line 116), outside the auto-migrate branch.
