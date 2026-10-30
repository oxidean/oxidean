## Review

**Verdict:** NOT-READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Date:** 2026-10-27T23:52:16Z
**Iteration:** 1
**Stage:** infrastructure-design
**Unit:** connector-gateway
**Artifact:** cicd-pipeline

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Major | cicd-pipeline.md — "`db-dialects -E` filter" row | FALSE mechanism + overstated wiring: the `-E` arg is a nextest inclusion filterset over `binary_id(...)`, not a test-name exclusion set, and `binary_id(oxidean-db::dialect_oxpilot)` is already in the union (`.github/workflows/ci.yml:886`; documented `docs/TESTING.md:309`). New tests in `crates/oxidean-db/tests/dialect_oxpilot.rs` run automatically — no filter edit required. The real constraint the row omits: tests MUST land in the already-listed binary; a new test binary would never match the filterset and would vacuously skip on `api-rust` (no `DATABASE_URL`) — silent zero real-engine coverage. The same false premise is recorded in `infrastructure-design-questions.md` Q2/Q4. | Rewrite the row: the filterset is a binary-id inclusion union already covering `dialect_oxpilot`; the same-change requirement is placing connector tests inside that shipped file (a NEW binary would require a `binary_id(...)` union term); `docs/TESTING.md` job-table row is the doc wiring. | New |
| R-02 | Major | cicd-pipeline.md — "`make rpc-gen` / `job: rpc-sync`" row | "no new proc verbs" is wrong about the shipped surface. Shipped `PROCEDURES`/dispatch (`crates/oxidean-api-oxpilot/src/lib.rs:29-42`, `dispatch.rs:22-41`) contain none of the 8 C1 procs (`pairing_create`, `target_list`, `target_get`, `target_disconnect`, `target_revoke`, `grant_list`, `grant_get`, `grant_respond`) — C1 pins them on paper; U6 registers them new. `rpc_gen.rs` emits per-proc client methods, so `packages/api-client` will diff and `check-rpc-sync.sh` (`git diff --exit-code`) fails unless regenerated output is committed — the wiring is unconditional, not "if codegen touched." | Correct the row: U6 adds ~8 new `oxpilot.*` verbs to `PROCEDURES`+dispatch; same-change requirement = run `make rpc-gen` and commit the regenerated client. Note the pin tests and `classify_pat_proc` (fail-closed `SessionOnly`) auto-cover new procs. | New |
| R-03 | Major | cicd-pipeline.md — Rollback step 2 | "Live connectors are unaffected by the flag (admission gate, U3 precedent)" contradicts BR2.5 (`functional-design/rules.md:68-71`): all four C3 routes fail closed `{error:"disabled"}` when enablement is unsatisfied — expressly so queued work cannot execute while the feature is off. Flag-off is a work-stop lever on LIVE connectors; it tombstones no state (verifier/targets/grants/queued actions persist; re-enable resumes). `target_delete` is also not a pinned C1 proc (`target_disconnect`/`target_revoke` are). | Rewrite step 2: flag-off refuses ALL C3 traffic including live connectors — work halts, state persists, re-enable resumes the fleet; disconnect/revoke own state teardown; fix `target_delete`→`target_disconnect`. | New |
| R-04 | Minor | cicd-pipeline.md — Rollback step 4; traceability.json | Single-replica boundary cites "(NFR-S1/NFR4.4)" — wrong ids. The pins are NFR-S2 ("single-replica caveat stated, not hidden") and NFR-S6 (no cross-node coordination); NFR-S1 is per-target serialization, NFR4.4 is report-closes-cancel_requested. `traceability.json` repeats the mis-mapping (NFR-S1→"single-replica boundary", NFR-S3→"no cross-node coordination", NFR-S4→"replicas>1 failure shape"). | Cite NFR-S2/S6 in the rollback note and repair the four `NFR-S*`/`NFR4.4` rows in `traceability.json`. | New |
| R-05 | Minor | infrastructure-specification.md — "deps" row | "none — tokio/tokio-util/sha2/uuid/oxidean-core all in-tree" is imprecise: `oxidean-api-oxpilot`'s manifest has no `sha2`, `rand`, or `uuid`, yet logical-components pins minting/hashing via workspace `sha2`+`rand`. Hashing can reuse api-core's public `sha256_hex`/`bytes_to_hex`, but no public token-mint helper exists (`rand` is api-core-internal). The manifest gains dependencies or minting moves into api-core — either way not "deps: none." | State honestly whether api-oxpilot adds `sha2`/`rand`/`uuid` to `[dependencies]` (no new external crates enter the lock) or whether mint/hash helpers land in api-core. | New |
| R-06 | Minor | infrastructure-specification.md — Environment/network posture | "existing `FailedAuthLimiter` IP buckets (connector auth + pairing-mint limiter)" mislabels the mint limiter: pairing-mint throttling is pinned per-USER (`check_user`/`record_user`, BR1.5/security-design); the shipped type carries both `by_ip` and `by_user` maps. "IP buckets" for the mint path could mislead the keying choice. | Distinguish IP-keyed connector-auth limiting from user-keyed pairing-mint limiting. | New |
| R-07 | Nit | cicd-pipeline.md — existing-CI table | `job: clippy`, `job: cargo-machete`, `doc-warnings`, `job: web-lint`, `job: web-format-check` are not job names — they are steps inside `dead-code` (ci.yml:232-255) and `web-octane` (ci.yml:271-281). | Rename to the actual job ids (`dead-code`, `web-octane`) or describe them as make targets within those jobs. | New |

> R-01 Reviewer note: Reviewer supplied prior ID R-01

> R-02 Reviewer note: Reviewer supplied prior ID R-02

> R-03 Reviewer note: Reviewer supplied prior ID R-03

> R-04 Reviewer note: Reviewer supplied prior ID R-04

> R-05 Reviewer note: Reviewer supplied prior ID R-05

> R-06 Reviewer note: Reviewer supplied prior ID R-06

> R-07 Reviewer note: Reviewer supplied prior ID R-07

### Validation Tool Results

No validation suite run — document review. Every claim verified against shipped surfaces: `.github/workflows/ci.yml` (job names, `-E` filterset, DATABASE_URL env, required-checks), `docs/TESTING.md` (lane table, ruleset policy, rpc-gen policy), `crates/oxidean-db/tests/dialect_oxpilot.rs` + `crates/oxidean-db/migrations/*/00*` (max 0087), `crates/oxidean-api-oxpilot/src/{lib,dispatch}.rs` + `Cargo.toml`, `crates/oxidean-api-core/src/{oxpilot.rs,audit.rs,pat/rate_limit.rs,auth/session.rs}`, `crates/oxidean-api/src/main.rs` (migrate-then-listen, hook order, `.nest("/api/oxpilot", …)`), `crates/oxidean-core/src/proc_classes.rs` (fail-closed SessionOnly), `crates/oxidean-web/src/proxy.rs` + `docker-compose.yml` + `deploy/cloud/Caddyfile` (`/api` prefix coverage), `.github/workflows/images.yml` + `docs/CONFIGURATION.md` + `.env.example`.

### Summary

NOT-READY — 3 Major, 3 Minor, 1 Nit (7 findings, all Unresolved). The artifact's core topology claims are true and verified: no new jobs/matrix/workflow files, unchanged deploy on the existing `api` image, next migration 0088 ×3 additive, honest single-replica deployment constraint, correct audit-helper split and limiter seams, no new env vars, `/api`-prefix coverage at every edge. Blocking issues: (1) the `db-dialects -E` row misdescribes the shipped nextest mechanism and asserts a filter extension that is already satisfied — while omitting the real same-change constraint (tests must live in the listed `dialect_oxpilot` binary); (2) "no new proc verbs" is false — ~8 C1 procs land in `PROCEDURES`, so `make rpc-gen` output must be committed or the required `rpc-sync` check fails; (3) the rollback plan claims live connectors are unaffected by flag-off, contradicting BR2.5's fail-closed pin — flag-off refuses all C3 traffic (state persists, re-enable resumes). Minor items: wrong single-replica NFR ids repeated into `traceability.json`, an imprecise "deps: none" claim vs missing `sha2`/`rand`/`uuid` in `oxidean-api-oxpilot`, a wrong "IP buckets" label on the per-user mint limiter, and job names cited for what are actually steps.
