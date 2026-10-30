# Tech-Stack Decisions — connector-gateway (U6)

Consumes: `construction/connector-gateway/functional-design/` artifacts,
`inception/requirements-analysis/requirements.md` (NFR1, NFR6, NFR9),
`inception/contract-design/contract-summary.md` (C1, C3, C5, C7, C8),
`codekb/oxidean-app/technology-stack.md`.

| id | decision | rationale |
|----|----------|-----------|
| TS1 | Gateway lives in `crates/oxidean-api-oxpilot` as a new `connector_gateway` module + `crates/oxidean-db` connector tables — no new crate | Shares SessionOnly proc registration, `SessionService` consumption (C5), and the existing DB access layer; a new crate would force a second proc domain for one feature (fd entities) |
| TS2 | Verifier/pairing hashing via `SHA-256` with the shipped find-by-hash model (`sha256_hex` lookup, bearer.rs precedent) — NOT an encryptable column; a `constant_time_eq` compare is acceptable surplus where a compare exists, but the shipped idiom is hash→lookup | Precedent exists in the auth layer; find-by-hash avoids a second secret-custody idiom (BR1.3, BR2.1, bearer.rs) |
| TS3 | Per-target admission lock via an in-process keyed async mutex — NOT DB `SELECT … FOR UPDATE` | Serializes dispatch/grant/close/dequeue-write in-process per BR4.1/BR5.1; the single-replica caveat is documented; keeps scans/reads lock-free |
| TS4 | Long-poll via in-process notification (tokio watch/notify per target) — no websockets, no SSE to connector | Outbound-only connector pins HTTP POST; a listener is out of scope (BR5.1/BR5.5, C3) |
| TS5 | Tri-dialect migrations in `crates/oxidean-db/migrations/{postgres,sqlite,mysql}`; `crates/oxidean-db/tests/dialect_oxpilot.rs` (already shipped + registered in `db-dialects -E oxpilot` and `docs/TESTING.md` by U1/U2) is EXTENDED for the connector tables and the TESTING.md row updated — all in the same change | NFR9's same-change clause; unregistered dialect coverage is dead coverage |
| TS6 | `audit_events` reuse via `audit::record` — no new audit table, honest about fire-and-forget delivery | The vocabulary lands on the existing events surface; no-FK posture is the pinned evidence-survival mechanism (BR7.2, entities, NFR5) |
| TS7 | No new dependencies beyond the workspace set: `tokio`, `serde`, the workspace's existing `sha2` primitive, `uuid`, `rand` (mint-path entropy — already in the workspace set via api-core) | NFR1/NFR6 — reuse keeps the trust surface identical to the shipped auth path |
| TS8 | Error vocabulary lands on existing codes — PROC side: `not_found`, `rpc.bad_input`, `forbidden`, `grant_not_pending`, `pairing_expired`, `oxpilot.disabled`; C3 route side: `unauthorized` (401 auth), `revoked`, `not_found` (404 report), `disabled`, `bad_input`, `pairing_expired`, `pairing_used`, `forbidden`, limiter 429 — no new error taxonomy, and the two domains' `unauthenticated`-vs-`unauthorized` split is deliberate | spec negative paths + C3 delta, BR6.3 |
