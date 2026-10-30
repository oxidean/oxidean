# Security Requirements — agent-runtime (U3)

Consumes: `construction/agent-runtime/functional-design/rules.md`
(BR1–BR6), `functional-design/entities.md`,
`functional-design/functional-spec.md`,
`inception/requirements-analysis/requirements.md` (NFR1, NFR2, NFR5),
`inception/contract-design/contract-summary.md` (C1, C2, C4, C5, C6),
`codekb/oxidean-app/technology-stack.md`.

The runtime sits between the SessionOnly proc layer and the provider
stream — it never authenticates callers itself, but it holds the seams
where secrets and per-user content could escape.

## Authentication & authorization boundaries

| id | requirement | basis |
|----|-------------|-------|
| NFR1.1 | `oxpilot.turn_cancel` is SessionOnly + `oxpilot.disabled`-gated like its siblings; a PAT/bearer refusal answers `auth.pat_scope`/`auth.unauthenticated`; a foreign `session_id` answers `not_found` — the runtime treats its proc surface identically to U2's (fd proc table) | NFR1, C1, fd proc table |
| NFR1.2 | The runtime invokes `SessionService` only under the session OWNER's `user_id` — captured from the authenticated request on the initial claim and carried FORWARD across chained claims (a chained turn has no request in flight); a turn lineage can never drift to another principal even if the owning session's row were somehow rebound | BR1.x, C5, C6 |
| NFR1.3 | Authorization for tool calls lives EXCLUSIVELY behind `ToolExecutor` (C6) — the runtime passes raw provider-emitted names/args through `executor.invoke`; it never pre-filters, whitelists, or executes a call the provider emitted outside the seam (BR3.1) | BR3.1, C6 |
| NFR1.4 | `ToolCtx` fields the runtime mints (`session_id`, `user_id`, `repo_id`, `proposal_id`) are populated from the turn's captured context — never from provider-emitted tool args; a model cannot self-escalate `proposal_id` or rebind `user_id` | C6, BR3.1 |

## Secret custody & prompt hygiene

| id | requirement | basis |
|----|-------------|-------|
| NFR1.5 | Provider key material never reaches the runtime — `turn_parts()` hands back `Arc<dyn LlmProvider>` whose construction (decrypt, build) happened inside U4's registry; the runtime holds the trait object only and has no accessor for keys (fd entities TurnParts — the widened `{provider, model, executor}` triple under one lock read per BR1.3) | NFR1, C4 |
| NFR1.6 | Tool-result content bound for the provider gets a second `oxidean_core::redact()` pass at the runtime boundary before it becomes `ChatMsg{role:"tool"}` — U2's write-boundary redaction covers user-authored rows; tool output is a NEW content path the service never saw (fd BR3.2) | BR7.2, NFR1 |
| NFR1.7 | No SSE frame carries secret-capable material: frames emit ids, deltas (transcript-bound content), card kinds, bounded `args_summary`/`output_summary` (already redacted + capped at `ARGS_SUMMARY_MAX = 2 KiB` / `OUTPUT_SUMMARY_MAX = 2 KiB`), terminal results — never `args_json` raw bodies, prompt context, or env values (fd entities constants) | C2, BR3.3, entities |
| NFR1.8 | Prompt assembly reads only the owner's transcript via `transcript(session_id, owner_user_id)` — the window is per-session rows already redacted at the U2 boundary; no cross-user content can enter a `ChatRequest` because the service refuses foreign reads at `not_found` | BR2.x, C5 |

## Surface integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR1.9 | `emitted_deltas` and `fanout` are process-local memory — they die with the turn; nothing in the buffer is a persistence path (transcript rows are the durable form) | entities ActiveTurn |
| NFR1.10 | `pending_content`/`pending_client_msg_id` live only inside `ActiveTurn` — never persisted, never emitted as a frame payload beyond the resend-match mechanism (fd BR2.x) | entities, BR2.x |
| NFR1.11 | `admission_lock` maps 1:1 to sessions and is dropped with the `ActiveTurns` entry — a lock can't outlive its session or leak across users | BR3.4, BR4.1 |

## Threat considerations

STRIDE-lite for agent-runtime: **Spoofing** — SessionOnly + owner-bound
`ToolCtx.user_id` (NFR1.1–1.4). **Tampering** — effective-`call_id`
minting preserves the `(session_id, call_id)` card key against providers
that omit ids; terminal CAS makes the outcome unrewriteable (fd BR3.2a,
BR5.x). **Repudiation** — terminal frames carry stable typed codes;
abort/cancel paths are distinguishable (`aborted` vs `error`)
(NFR5.x, reliability NFR4.x). **Information disclosure** — key material
behind the provider Arc, second redact pass on tool results, capped
summaries in frames, `not_found` anti-enumeration on `turn_cancel`
(NFR1.5–1.8). **Denial of service** — `TURN_MAX_DURATION` wall cap,
`TOOL_CALL_TIMEOUT = 60 s` per call, `MAX_TOOL_ROUNDS = 8` exhaustion bound,
prompt-window cap, per-tool output cap (performance NFR3.x). **Elevation
of privilege** — tool authz is exclusively `ToolExecutor`; the runtime
adds no side-channel exec path (NFR1.3).

## Privacy

| id | requirement | basis |
|----|-------------|-------|
| NFR2.1 | The turn task logs and spans carry ids only (`session_id`, turn `generation`, `call_index`, drained counts) — never prompt text, deltas, tool args/results, or user identifiers beyond the owner session id (observability NFR6.x, NFR5) | NFR5, NFR6 |
| NFR2.2 | `pending_content` is the already-REDACTED in-memory resend-match copy (routes.rs redacts before the field is set) — transient, never logged, never persisted; the durable user row redacts at the U2 service boundary | entities, NFR1 |
