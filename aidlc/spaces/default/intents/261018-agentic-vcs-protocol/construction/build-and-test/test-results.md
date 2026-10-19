# Test Results — agentic-vcs-protocol

Executed 2026-10-19 against the floor-19 code-generation state (all 10 units
approved). Every command from the instruction files ran once; no failures on
any exit path.

## Build status

- `cargo check --workspace` — clean (client + server feature trees compile).
- `cargo clippy -p oxidean-vcs -p oxidean-cli -p oxidean-api --all-features` — clean.
- `bun run --filter @oxidean/web build` artifacts pre-date this run; web gates
  below all pass. **Build: success.**

## Test results

| Suite | Command | Result |
|-------|---------|--------|
| Workspace unit+integration | `cargo nextest run --workspace` | **1529 run: 1529 passed, 3 skipped** (155 binaries, 16.6s) |
| Web unit+integration | `bun run --filter @oxidean/web test` | **609/609** (133 files, 207s) |
| Web browser (Chromium) | `bun run --filter @oxidean/web test:browser` | **78/78** (34 files, 91s) |
| Web lint | `make web-lint` | ok — type-aware oxlint + octane DOM-race + anchor + route-sync all pass |
| Web format | `make web-format-check` | ok — 505 files Prettier-clean |
| Generated-client drift | `make rpc-sync-check` | ok |
| Client feature boundary | `cargo tree -p oxidean-vcs --no-default-features --features client` | 0 `oxidean-db`/`oxidean-git`/`sqlx` deps |
| Migration dialect parity | `ls crates/oxidean-db/migrations/{postgres,mysql,sqlite}` | 0053–0055 ×3 dialects, filename parity |

Per-unit tallies inside the workspace run (all passing): `oxidean-vcs` 207
(server + client incl. 29 U8 client tests), `oxidean-cli` 155 (130 lib + 25
e2e incl. `vcs.*` wire-shape asserts), `oxidean-api` full suite incl. vcs
dispatch/object-route/MCP wiring, `oxidean-web` full suite. Web additions:
`vcs-settings-panel` 8 integration + 2 browser, `repo-activity-row` 6
integration.

## Failure details

None. One parallel flake was observed during an engine-run verify earlier in
the session (`factory_reset_scope`, unrelated to VCS) — it passed standalone
and on retry; not a code defect.

## Coverage report

No statement-coverage instrumentation this cut (recorded posture); the
cross-unit requirement coverage gate reports **160/160** IDs covered — see
cross-unit-traceability.md.

## Target Verification Matrix (finalized)

| Target ID | Source | Expected | Actual | Evidence | Owning Stage | Verdict |
|-----------|--------|----------|--------|----------|--------------|---------|
| T-NFR1 | requirements.md#NFR1 | flat visit counts; caps injectable | `log_seen_cap_trips`, `merge_base_mark_cap_trips`, `diff_candidates_cap_trips`, `blame_touches_cap_trips` PASS | nextest output (oxidean-vcs read_index) | build-and-test | Met |
| T-NFR2 | requirements.md#NFR2 | concurrent creates on one parent succeed | concurrent-create tests PASS | nextest (changeset_store/ref_registry CAS family) | build-and-test | Met |
| T-NFR3 | requirements.md#NFR3 | immutable content-addressed objects | `put_roundtrip_dedup`, `verify_detects_corruption`, `put_empty_object` PASS | nextest (object_store) | build-and-test | Met |
| T-NFR4 | requirements.md#NFR4 | stateless object/read routes | object GET + vcs.* read tests PASS | nextest (oxidean-vcs api, oxidean-api) | build-and-test | Met |
| T-NFR5 | requirements.md#NFR5 | version gate unchanged | manifest advertises vcs family; `Oxidean-RPC-Version` gate tests PASS | nextest (oxidean-api) + `required_procedures` fail-closed tests | build-and-test | Met |
| T-NFR6 | requirements.md#NFR6 | 3-dialect migration parity | 0053–0055 exist in postgres/mysql/sqlite | `ls crates/oxidean-db/migrations/*` | build-and-test | Met |
| T-NFR7 | requirements.md#NFR7 | per-object resumable; no cache corruption | object_cache verify/refetch + working_copy temp-rename tests PASS | nextest (oxidean-vcs client, 29 tests) | build-and-test | Met |
| T-NFR8 | requirements.md#NFR8 | typed `vcs.*` errors, no internals leak | typed-code assertions across api/cli e2e PASS | nextest (oxidean-api, commands_e2e) | build-and-test | Met |
| T-NFR9 | requirements.md#NFR9 | no cookies; PAT ACL; PUT size/quota | object-route auth tests + `put_repo_quota_denies_link_keeps_object` PASS | nextest (oxidean-vcs, oxidean-api) | build-and-test | Met |
| T-SR3 | u1 SR3 | conflicts ≤500 + truncated flag | conflict-cap tests PASS | nextest (merge_engine) | build-and-test | Met |
| T-SR4 | u4 SR4.1 | path/mode/author/message/parents bounds | validation test family PASS | nextest (changeset_store) | build-and-test | Met |
| T-C1LIM | contract C1 | limit ≤500; changes ≤10_000 | handler clamp/reject tests PASS | nextest (oxidean-vcs api) | build-and-test | Met |
| T-OBJCAP | u3 SR | 256 MiB mid-stream abort; quota probe | `put_object_quota_aborts_tmp`, `put_rereference_no_quota_charge` PASS | nextest (object_store) | build-and-test | Met |
| T-TTL | u6 SR6.5 | marker TTL 900s; nonce-verified release | interop lock/claim tests PASS | nextest (git_interop) | build-and-test | Met |
| T-CLI-EXIT | contract C8 | auth codes → exit 3 | `error_exit_code` + e2e tests PASS | nextest (oxidean-cli) | build-and-test | Met |
| T-REFNAME | contract C1 | `ref_name` on wire req+resp | e2e asserts `"ref_name"` in body and render | nextest (commands_e2e) | build-and-test | Met |
| T-MCP | contract C3 | 13 tools; owner+name required | 5 descriptor + 4 host-wiring tests PASS | nextest (oxidean-vcs mcp, oxidean-api mcp) | build-and-test | Met |
| T-WEB | contract C4 | toggle panel + vcs.* rows + chips | 14 integration + 2 browser tests PASS | vitest/browser output | build-and-test | Met |
| T-BOUNDARY | ADR-011 | client feature: no server deps | 0 `oxidean-db`/`oxidean-git`/`sqlx` in client tree | `cargo tree` output | build-and-test | Met |
| T-WS | testing posture | workspace all pass | 1529/1529, 3 skipped | nextest summary | build-and-test | Met |
| T-WEBSUITE | testing posture | web suites pass | 609 vitest + 78 browser + lint/format ok | vitest/make output | build-and-test | Met |
| T-RPCSYNC | affirmed rule | no generated-client drift | `rpc-sync-check: ok` | make output | build-and-test | Met |
| T-XUNIT | step 10 | every FR/NFR/AC covered | 160/160 IDs with existing target files | cross-unit-traceability.md | build-and-test | Met |

No `Pending` verdicts remain; no `Not Met`/`Unverified` targets.

## Loop-Back Log

Not applicable — no rung-3/4 loop-backs fired; the stage completed on its
first execution pass.
