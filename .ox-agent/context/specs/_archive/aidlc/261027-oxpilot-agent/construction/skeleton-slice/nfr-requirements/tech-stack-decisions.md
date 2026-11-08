# Tech Stack Decisions — u1-skeleton-slice

Consumes: `codekb/oxidean-app/technology-stack.md`,
`construction/skeleton-slice/functional-design/` (all),
`inception/requirements-analysis/requirements.md` (constraints block),
`inception/contract-design/contract-summary.md` (C1–C4).

Every selection is an existing-stack reuse; the skeleton adds zero new
dependencies and zero new infrastructure.

## Server

| decision | selection | rationale |
|----------|-----------|-----------|
| RPC surface | New `oxidean-api-oxpilot` domain crate: `PROCEDURES` + `dispatch` arm + `extend_from_slice` in the shell | mandated pattern (FR6.2); `oxidean-api-issues` is the canonical skeleton |
| Streaming transport | Dedicated SSE routes under `/api/` — `Sse` + `KeepAlive` + mpsc fan-out, the `stream_job_log` precedent | FR6.3, C2; typed-RPC dispatch cannot stream |
| Provider abstraction | `LlmProvider` trait object on `RpcCtx` (`Option<…>` slot, the `vcs` precedent); U1 ships `StubProvider` | C4; slot resolution via `OXIDEAN_OXPILOT_PROVIDER` (BR2.5) |
| Turn coordination | `ActiveTurn` — concurrent map (`DashMap`-style or `Arc<Mutex<HashMap>>`) on `RpcCtx` + per-turn `tokio::broadcast` fan-out + delta ring buffer | entities; matches in-proc limiter precedent (FailedAuth/Lookup) |
| Persistence | `oxidean-db` `oxpilot_sessions` + `oxpilot_messages`, migrations ×3 + `dialect_oxpilot.rs` in the `db-dialects -E` filter | FR6.6, BR1.4 |
| Auth | existing session-cookie gate + `proc_classes` SessionOnly-by-omission | NFR1, BR1.2/BR5.2 |
| Redaction util | New `redact()` in `oxidean-core` (pattern-table detector; the runner's `mask_secrets` is known-value masking and doesn't transfer) — U1 authors it, U2+ reuse | R-03 finding, BR3.5 |
| Error surface | `AppError{code}` domain codes — `oxpilot.disabled` on gated surfaces, `not_found` (existing shared code) for anti-enumeration/ownership denials, `turn_active` on the 409 | C1/C2, code-style |

## Web

| decision | selection | rationale |
|----------|-----------|-----------|
| Route | Astro shell + `.tsrx` under the app shell; `RESERVED_TOP_SEGMENTS += "oxpilot"`; routeTree regenerated | BR5.1, check-route-sync triangle |
| Data layer | `oxpilot-queries.ts` TanStack queryOptions factory + `usePageRedirect` auth gate, mirroring `settings/ai.tsrx` | code-style, BR5.2 |
| Stream client | fetch `ReadableStream` reader parsing SSE frames for POST-chat (EventSource is GET-only); GET stream may use either | refined-mockups learning, C2 |
| Chat client | `@tanstack/ai-octane` `useChat`-family — mandated framework; skeleton uses it for session/composer state while the wire stays our C2 frames | constraint, FR6.1 |

## Usability & accessibility baseline (skeleton share)

| id | requirement | basis |
|----|-------------|-------|
| NFR7.1 | The minimal page still meets baseline semantics: a `main` landmark, ordered headings, and labelled form controls — aria-live/stream choreography is U8 scope | NFR7, BR5.3 |
| NFR8.1 | The flag-off state names `OXIDEAN_OXPILOT_ENABLED` + provider env vars + `docs/CONFIGURATION.md` — explanatory, never a dead page; errors name the fix | NFR8, BR5.4 |
| NFR8.2 | Typed errors carry the fix: `oxpilot.disabled` carries `reason`, `409` carries `turn_active` — the UI renders actionable copy from codes, not stacks | NFR8, C1/C2 |

## Explicitly not selected (and why)

| rejected | why |
|----------|-----|
| New metrics/exporter stack | NFR6 forbids new observability infra |
| Redis/shared turn lock | single-replica v1; documented caveat (NFR3.13) |
| WebSocket for chat | SSE precedent exists end-to-end (compression exclusion already shipped); WS adds a second real-time stack |
| React JSX | forbidden — Octane `.tsrx` only |
| Provider SDKs (openai/anthropic) | U1 ships the stub only; adapters land in U4 behind the same trait |
