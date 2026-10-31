# Tech Stack Decisions — session-core (U2)

Consumes: `codekb/oxidean-app/technology-stack.md`,
`construction/session-core/functional-design/*`,
`inception/requirements-analysis/requirements.md` (NFR9).

Stack is inherited — this unit adds no new dependencies. Decisions
below are stack-conformance constraints: how existing stack pieces get
used. Ids run `NFR10.x` — NFR7/NFR8/NFR9 numbers are reserved for the
upstream domains they name (accessibility, usability, testability) and
this file's rows are engineering constraints, not new upstream NFRs
(review R-12).

## Data layer

| id | decision | basis |
|----|----------|-------|
| NFR10.1 | New tables (tool_calls, queued_messages, proposals, usage_counters) ship in migration `0087_oxpilot_session_core.sql` in ALL THREE dialects (postgres/mysql/sqlite) — the `oxidean-db` dialect test extends to cover them in the same change | FR6.6, NFR9, entities |
| NFR10.2 | The dialect test update and the `docs/TESTING.md` `-E` filter row ship in the SAME change as the migration — no orphan migration | NFR9, FR6.6 |
| NFR10.3 | `oxidean-db` remains the ONLY crate with dialect branches — `oxidean-api-oxpilot` calls the store's async fns; no sqlx/diesel import crosses the crate line | stack.md, BR7.1 |

## Service layer

| id | decision | basis |
|----|----------|-------|
| NFR10.4 | `SessionService` + `SessionDeleteHook` are traits in `oxidean-api-core` under the `oxpilot` module — the full path `oxidean_api_core::oxpilot::SessionService` is DISAMBIGUATED from the existing `oxidean_api_core::auth::session::SessionService` struct (api-core/src/auth/session.rs); shared context files (rpc.rs, oxpilot.rs) that could name both use the qualified path. The concrete `OxpilotSessionService` lives in `oxidean-api-oxpilot::services`; the RUNTIME (`OxpilotRuntime`, not the slot — api-core's boot constructor can't name the impl) holds `OnceLock<Arc<dyn oxpilot::SessionService>>` installed once by the shell post-construction — keeps the dependency direction api-core ← api-oxpilot. Refines fd BR7.1 (review R-07; residence refined at nfr-design R-15) | C5, C8, BR7.1, review R-07 |
| NFR10.5 | `oxidean_core::redact()` is the only detector — U2 adds no second redactor and no per-call-site pattern list | stack.md, BR3.1 |

## Queue & concurrency

| id | decision | basis |
|----|----------|-------|
| NFR10.6 | Queue uses a real table (`oxpilot_queued_messages`), not an in-memory channel — crash-surviving persistence is the contract; the `ActiveTurns` map stays process-local per U1 | entities, BR4.x |
| NFR10.7 | `queue_seq` collision retry is a bounded loop inside `enqueue` — at most 8 mint attempts (concurrency degree is realistically 2–3); exhaustion surfaces the storage error, never an infinite retry | BR4.1 |

## Testing

| id | decision | basis |
|----|----------|-------|
| NFR10.8 | Closed-set `PROCEDURES` pin test updates for +5 procs — collect-only coverage, not a separate registry | NFR9 |
| NFR10.9 | `dialect_oxpilot` test extends to all four new tables — one test function per dialect branch pair, same shape as U1's | NFR9, BR7.1 |
| NFR10.10 | Integration tests hit a real sqlite-backed app harness — hook probe uses a test-only `SessionDeleteHook` impl inside the crate's `#[cfg(test)]` module (no production registrar shipped) | NFR9, C8 |
