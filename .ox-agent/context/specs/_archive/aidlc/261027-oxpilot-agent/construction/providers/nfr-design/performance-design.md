# Performance Design — providers (U4)

Consumes: `nfr-requirements/performance-requirements.md` (NFR3.1–3.8).
Questions: `nfr-design-questions.md` Q4–Q6.

## Hot-path reads (NFR3.1–3.2)

```rust
// crates/oxidean-api-core/src/oxpilot.rs — slot read, zero DB on requests
pub struct OxpilotRuntime {
    pub slot: tokio::sync::RwLock<OxpilotRuntimeSlot>,
    pub rebuild_lock: tokio::sync::Mutex<()>,
}
// OxpilotRuntimeSlot { provider: Option<Arc<dyn LlmProvider>>,
//   model: Option<String>, disabled_reason: Option<String>,
//   turns: Arc<ActiveTurns> }  — turns is a slot field only; nothing
//   else carries it

async fn resolve_oxpilot(rt: &OxpilotRuntime) -> OxpilotRuntimeSlot {
    rt.slot.read().await.clone()   // Option<Arc<provider>> + reason — no DB
}
```

The slot IS the resolved projection — `oxpilot_settings` is read only
inside a rebuild (boot + `settings_set`), never per request, frame, or
status call. `resolve_oxpilot` clones `Option<Arc>` + small fields; the
`turns` field clones the `Arc<ActiveTurns>` handle.

## Stream path (NFR3.3)

Provider frames decode incrementally into `ChatStreamItem` as bytes
arrive — `Response::bytes_stream()` feeds a byte buffer;
`SseDecoder` splits on `\n\n` event boundaries (data: lines →
`ChatStreamItem`) and `NdjsonDecoder` splits on `\n`. `futures_util`'s
`StreamExt` drives the chunks — `BufRead::lines` is std-blocking and
does not apply to reqwest's async stream. No whole-response buffering,
no extra payload copy. Deltas pass through to the turn's broadcast
channel at U1's existing cost.

## Client efficiency (NFR3.4–3.5)

- One `reqwest::Client` per adapter construction — DNS/TLS/connection
  pool shared across every turn that adapter serves.
- `decrypt_key` runs once inside `providers::build`; the adapter holds
  only the header-ready material. No per-turn crypto.
- Provider construction makes NO network call — `base_url` validation
  (one `lookup_host`) is the only network touch, once per build.

## Rebuild path (NFR3.6–3.7)

`rebuild` = one settings-row read + one `lookup_host` (when a
base_url is set) + local adapter construction + O(1) Arc swap. It runs
under the rebuild mutex, off the request path; `settings_set`'s extra
cost is the settings write + audit row.

## Boundedness (NFR3.8)

`providers_list` iterates the static 3-entry REGISTRY plus ONE
`oxpilot_settings` singleton-row read (key-presence for `configured`)
— constant cost, no fan-out, no unbounded scan.
