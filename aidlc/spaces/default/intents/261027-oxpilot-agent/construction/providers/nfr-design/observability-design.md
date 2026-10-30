# Observability Design — providers (U4)

Consumes: `nfr-requirements/observability-requirements.md`
(NFR5.1–5.3, NFR6.1–6.6). Questions: `nfr-design-questions.md` Q7.

## Audit (NFR5.1–5.3)

```rust
// crates/oxidean-api-oxpilot/src/settings.rs
async fn audit_oxpilot_settings(ctx: &RpcCtx, action: &str, fields: &[&str]) -> Result<()>;
// oxidean-db audit table: {actor_user_id, action, detail: {fields: names}, ts}
// invoked inside the rebuild mutex AFTER the slot swap — audit order
// matches commit order; a persist→audit crash can lose the row
// (documented bounded gap, NFR1.12)
// call site swallows the error into warn! — a failed audit write never
// makes a committed settings change read back as a request failure
```

`field_names` enumerates only the payload keys present
(`provider_id`, `base_url`, `api_key`, …) — the string `"api_key"`
itself is the most sensitive thing audited; no value ever lands.

## Tracing & logging (NFR6.1–6.4)

- Provider calls: `tracing::info_span!("oxpilot.provider", provider_id, model)`
  wrapping `stream()` — payload, base_url, and key material never span
  attributes.
- Provider failure: one warn log per terminal — `provider_id`, session id,
  turn id, `ProviderError.code`; the same code rides the C2 terminal frame.
- Boot: one info log of the resolved reason (`enabled` / `flag_off` /
  `no_provider` / `misconfigured`); unrecognized
  `OXIDEAN_OXPILOT_PROVIDER` values warn distinctly — the operator-facing
  misconfig signal.
- Secret hygiene: `oxpilot_key`'s decrypt is construction-only; tests
  assert ciphertext is the only persisted/emitted form (NFR6.4).

## Diagnostic surfaces (NFR6.5–6.6)

- `oxpilot.status` → `{enabled, provider{id,model}|null, reason}` —
  gate-exempt operator diagnostic.
- `admin.oxpilot.settings_get` → effective values + `*_overridden` flags
  — distinguishes DB override from env default at a glance.
- `admin.oxpilot.providers_list` → `{id, models, configured}` — the
  "selectable now" readiness signal per adapter.
