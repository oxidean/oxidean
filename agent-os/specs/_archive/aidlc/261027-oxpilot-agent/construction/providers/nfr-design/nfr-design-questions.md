# NFR Design Questions — providers

Unit: `providers` (U4). NFR requirements already approved
(`nfr-requirements/`); this stage pins the concrete mechanisms.

## Q1. Custody helper shape?

- A. Thin `oxpilot_key` helpers in `oxidean-api-oxpilot`:
  `encrypt_key(plaintext) -> Result<(ciphertext_hex, last4)>` and
  `decrypt_key(ciphertext) -> Result<String>` wrapping
  `oxidean_api_core::crypto::encrypt/decrypt` with
  `KEY_ENVS = ["OXIDEAN_OXPILOT_SECRETS_KEY","OXIDEAN_SESSION_SECRET"]` —
  decrypt callable only from the adapter-construction path (module
  privacy keeps it off read paths).
- B. Call `oxidean_api_core::crypto` directly from every site — duplicates the
  KEY_ENVS constant and lets decrypt drift onto read paths.
- X. Other (please specify)

[Answer]: A — thin custody helpers in `src/providers/key.rs` wrapping `oxidean_api_core::crypto` under the OXPILOT key chain; `decrypt_key` is `pub(in crate::providers)` so only the construction path can open the key. Conductor-decided (2026-10-27); ratify at PR review.

## Q2. SSRF validator shape?

- A. `validate_base_url(provider_id, url) -> Result<()>` in the crate:
  parse → scheme http(s) → host classify; non-ollama ⇒ literal-IP forms
  (std `IpAddr` parse covers IPv4/IPv6/IPv4-mapped; hand-check hex/
  octal/integer literals first since `IpAddr::from_str` rejects them)
  + `tokio::net::lookup_host` — EVERY answer must be public (private/
  loopback/link-local/0.0.0.0/multicast/169.254.169.254 metadata =
  reject). `ollama` skips the private check. Returns `invalid_provider`.
- B. Reject all non-public suffixes via a denylist regex — brittle and
  misses mapped forms.
- C. Defer to connect-time — TOCTOU is worse; validation must happen
  before the row commits.
- X. Other (please specify)

[Answer]: A — `validate_base_url(provider_id, url)`: http(s) + literal-IP-form classify + `lookup_host` all-public; `ollama` exempt; failure ⇒ `invalid_provider`. Conductor-decided (2026-10-27); ratify at PR review.

## Q3. Registry organization?

- A. Static `REGISTRY: &[ProviderDescriptor]` (id, models, requires_key)
  + `build(cfg) -> Result<Arc<dyn LlmProvider>>` dispatching over a
  4-member RESOLUTION_MAP = REGISTRY + stub. `providers_list` maps
  REGISTRY (3, stub omitted); `settings_set` refuses `stub` unless
  `cfg!(debug_assertions)`; env `stub` stays legal in every build
  (U1 parity); unknown id ⇒ `invalid_provider`.
- B. Dynamic registration map — open-world invites plugin-style drift;
  the contract pins a closed set.
- X. Other (please specify)

[Answer]: A — static 3-entry REGISTRY + `build(cfg)` over the 4-member RESOLUTION_MAP (adds env-path stub); `providers_list` reads REGISTRY + one settings row for `configured`; `settings_set` refuses `stub` unless `cfg!(debug_assertions)`; unknown id ⇒ `invalid_provider`. Conductor-decided (2026-10-27); ratify at PR review.

## Q4. Slot holder + rebuild mechanics?

- A. `RpcCtx.oxpilot: Option<Arc<OxpilotRuntime>>` where
  `OxpilotRuntime{slot: RwLock<OxpilotRuntimeSlot>, rebuild_lock:
  tokio::sync::Mutex<()>}` (types in api-core; all construction logic in
  api-oxpilot — one-way dep). `rebuild(&state)` runs
  resolve→validate→build→swap reusing `Arc<ActiveTurns>`; `main.rs`
  calls it once unconditionally after `AppState::new`; `settings_set`
  runs the same helpers + persist + audit inside the same mutex.
- B. `arc_swap::ArcSwap` — new dep for a writer-rare read-heavy slot;
  RwLock is fine.
- X. Other (please specify)

[Answer]: A — `OxpilotRuntime{slot: RwLock<OxpilotRuntimeSlot>, rebuild_lock: Mutex}` held as `Option<Arc<OxpilotRuntime>>` on RpcCtx; `rebuild` resolve→build→swap reusing `Arc<ActiveTurns>`; unconditional post-`AppState::new` boot call in main.rs + `settings_set` path. Conductor-decided (2026-10-27); ratify at PR review.

## Q5. DB-aware enablement path?

- A. `resolve_oxpilot(ctx)` reads the RwLock'd slot (no per-request DB);
  `require_enabled` maps `provider:None`+reason → `oxpilot.disabled`;
  `oxpilot.status` + `admin.oxpilot.*` skip the gate. U1's sync
  env-only path is replaced by the slot read in the same PR (BR5.5).
- B. Async DB read per request — NFR3.2 forbids it.
- X. Other (please specify)

[Answer]: A — `resolve_oxpilot` reads the slot, no per-request DB; `status`/`admin.oxpilot.*` gate-exempt; U1's env-only path replaced in the same PR (BR5.5). Conductor-decided (2026-10-27); ratify at PR review.

## Q6. Adapter stream decode?

- A. Shared `SseDecoder` (OpenAI+Anthropic SSE `\n\n` framing) +
  `NdjsonDecoder` (Ollama `\n` framing) fed by
  `Response::bytes_stream()` + `futures_util::StreamExt`; each adapter
  maps frames to `ChatStreamItem::{Delta,ToolCall,Done,Error}` and
  classifies HTTP failures into `ProviderError.code` via one
  per-adapter table.
- B. Per-adapter decoders — three copies of the same framing logic.
- X. Other (please specify)

[Answer]: A — shared `SseDecoder`/`NdjsonDecoder` over `bytes_stream()` chunks; per-adapter classify to `ChatStreamItem` + `ProviderError.code` table. Conductor-decided (2026-10-27); ratify at PR review.

## Q7. Audit + tracing wiring?

- A. `audit_oxpilot_settings(ctx, action, &[field_names])` writing
  {actor, action, names, ts} inside the mutex section (after swap);
  provider calls wrapped in `tracing::info_span!("oxpilot.provider",
  provider_id, model)` — no payload/key attrs; `ProviderError` terminal
  logs code+ids once at warn.
- B. Full request/response tracing — leaks prompts and keys (NFR1.7).
- X. Other (please specify)

[Answer]: A — `audit_oxpilot_settings` field-names-only inside the mutex section; `info_span` with provider_id+model attrs only; ProviderError warn-log once. Conductor-decided (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Custody: thin encrypt/decrypt helpers, decrypt pinned to construction
  (Q1); SSRF validator per BR3.2 with literal-form + DNS checks (Q2).
- Registry: 3-entry REGISTRY + 4-member RESOLUTION_MAP (env-stub legal,
  write-path stub debug-only) (Q3).
- Slot: OxpilotRuntime types in api-core, rebuild logic in api-oxpilot;
  unconditional boot rebuild + settings_set critical section (Q4).
- Gate: slot read, no per-request DB (Q5).
- Decode: shared SSE/NDJSON decoders + per-adapter classify tables (Q6).
- Observability: names-only audit in-section; attr-only spans (Q7).

Does this all look correct before I write the NFR design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
