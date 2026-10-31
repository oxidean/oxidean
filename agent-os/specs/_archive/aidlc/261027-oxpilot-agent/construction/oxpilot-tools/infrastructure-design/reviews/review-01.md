### Review — infrastructure-design / oxpilot-tools

**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** NOT-READY

### Review

### Prior findings

| ID | Prior finding | Resolution |
|----|---------------|------------|
| — | First iteration — no prior findings | — |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Major | `infrastructure-specification.md` §Deployable surface (l.18); `cicd-pipeline.md` §Build & test stages (l.17, l.19) | The three user procs are named `oxpilot.diff_{get,apply,comment}` in all three places — and l.19 even pins that spelling into the rpc-gen generated-client claim. `diff_comment` exists nowhere in the intent (`grep -rn "diff_comment" aidlc/` → zero hits). C1 pins the family as `diff_list`, `diff_get`, `diff_apply` (`inception/contract-design/contract-summary.md:46`; restated `functional-design-questions.md:30`, `functional-spec.md:16`, `nfr-design/logical-components.md:20` — the `tools/procs.rs` row names `oxpilot.diff_list`, `oxpilot.diff_get`, `oxpilot.diff_apply` — and `nfr-design/security-design.md:153`). The confusion is plausible — `pr_comment` is the MODEL-side catalog tool (`tools/comment.rs`, logical-components.md:19) and `oxpilot.artifact_comment` is an audit EVENT (observability-design.md:59) — but neither is a user proc. Followed literally, codegen registers `oxpilot.diff_comment` on `PROCEDURES`, rpc-gen generates `diffComment` into `packages/api-client` (the drift check passes — it only checks Rust↔TS sync, not contract names), and the C1-pinned `diff_list` the diff-review flow needs is absent; no automated gate catches a wrong-but-consistent name. | Change `comment` → `list` in all three places (`diff_{list,get,apply}`), matching contract-summary.md:46 and logical-components.md:20. | Open |
| R-02 | Minor | `monitoring-design.md` §Operator signals, `internal` ToolError row | The row claims `internal` ToolError ⇒ "`warn!` via `invoke_call`'s existing path". Shipped code says otherwise: `invoke_call`'s `Err` arm becomes `status="error"` (runtime.rs:585–588) and is logged by `tracing::debug!` (runtime.rs:620–626) with fields `{session_id, tool, call_index, status}` — no `warn!` anywhere on the tool-result path and no `code` field. Upstream `observability-design.md:111` asserts the same misstatement ("invoke_call logs tool errors at warn with `{session_id, tool, code}` (runtime.rs)"), so the defect is inherited, not introduced. Consequence: an unexpected handler failure logs at `debug` — invisible at the default `info` level; only the durable tool-call row captures it. | Correct the row — either pin a `warn!` inside the U5 handler before returning `internal` (say so explicitly), or state that tool errors land at `debug!` and the durable tool-call row is the signal. | Open |

### Verified claims (no defect)

- **No deployable surface** — the crate exists (`crates/oxidean-api-oxpilot/`); `tools/` does not yet exist, consistent with "eight `tools/*.rs` modules" being new code; every referenced seam is real: `ToolExecutor` trait + `slot.tools: Option<Arc<dyn ToolExecutor>>` (`oxidean-api-core/src/oxpilot.rs:117,1046`), `NullTools` fallback (:1236), `install_sessions`/`install_gateway` (`oxpilot.rs:1192,1207`; called at `oxidean-api/src/main.rs:143–144` — exactly where the spec says the install goes), `emit_frame` `pub(crate)` (`oxidean-api-oxpilot/src/runtime.rs:113`), `record_with` (`oxidean-api-core/src/audit.rs:25`, swallows failures as claimed), `invoke_call` (`runtime.rs:438`), `require_enabled` (`oxpilot.rs:1324`, returns `oxpilot.disabled`), `require_verified` (`auth/gate.rs:23`), `turns().get` (`oxpilot.rs:1223` → `ActiveTurns::get` :966), `dispatch_action`/`action_status`/`list_targets` (`oxpilot.rs:736,746,766`; gateway impls `connector/gateway.rs:553,696,796`), `save_proposal`/`mark_applied`/`proposal`/`get_session` (trait :492–504, impls `services.rs:625,658,650,232`), `find_user_by_id` → `UserRow.username` (`oxidean-db/src/lib.rs:3131`, `users.rs:12`), `rpc_dispatch` on `RpcCtx`/`AppState` (`rpc.rs:170`, `app.rs:98`), `build_rpc_ctx_with_session` (`app.rs:537`), `AppState: Clone` with `pub oxpilot` (`app.rs:23–24,93`). `ClientMeta::default()` exists (`rpc.rs:60–61`).
- **Install survives settings rebuilds** — every rebuild path carries `prior.tools.clone()` (`settings.rs:463,471,498,550,562,670,684`), so the one-time slot install at shell assembly is durable.
- **`0089` is the next migration number ×3** — `0088_oxpilot_connector.sql` is the latest in `migrations/{postgres,mysql,sqlite}/` identically; `oxpilot_proposals` exists (`0087_oxpilot_session_core.sql:62`, no `ops_json` column); the filename-parity test enforces ×3 presence (`crates/oxidean-db/src/migrate.rs:89–106`); `sqlx::migrate!` is append-only/checksum-validated and runs at api boot by default (`main.rs:68–77`, `OXIDEAN_AUTO_MIGRATE` opt-out).
- **`dialect_oxpilot` already in the `db-dialects` `-E` filterset** — ci.yml:902: `binary_id(oxidean-db::dialect_probe) | … | binary_id(oxidean-db::dialect_actions) | binary_id(oxidean-db::dialect_oxpilot)`; the binary exists (`crates/oxidean-db/tests/dialect_oxpilot.rs`) and `db.migrate()` runs the full dir, so `0089` is exercised per-dialect. (`docs/TESTING.md:309`'s `-E` text is stale — omits `dialect_actions`, uses `or` vs `|` — a pre-existing repo doc drift, not this artifact's defect; the `dialect_oxpilot` row it claims is present.)
- **`api-rust` replays the workspace archive with no `-E`** — ci.yml:291–297 uses only `--partition count:N/5`; new unit/proc tests ride automatically.
- **`rpc-gen`/`rpc-sync` gate is real** — `Makefile:96–106` (`rpc-gen` → generated `packages/api-client`; `rpc-sync-check` → `scripts/check-rpc-sync.sh` = `make rpc-gen` + `git diff --exit-code -- packages/api-client`); the `rpc-sync` job runs it (ci.yml:667).
- **"Zero new jobs ⇒ no `Main` ruleset edit" is sound** — `docs/TESTING.md:324` triggers only on job add/rename/remove or matrix-value changes; U5 adds none.
- **Zero new env vars** — the `OXIDEAN_OXPILOT_*` chain is already shipped (`settings.rs:64–80`; `.env.example:166–168`; `docs/CONFIGURATION.md:398–402`); `SettingsSetRequest.enabled_override` exists (`settings.rs:220,299`), so the `admin.oxpilot.settings_set{enabled_override:false}` rollback path works.
- **Bounded-work pins match upstream** — O(23) catalog (23 = 15 read + 2 propose + 5 dispatch + 1 comment per logical-components.md:16–19; `functional-spec.md:309`), validator depth ≤16 vs authored ≤3, `ACTION_ARGS_MAX = 2*1024` (`oxpilot.rs:541`), `TOOL_OUTPUT_MAX` 32 KiB / `OUTPUT_SUMMARY_MAX` 2 KiB (`runtime.rs:41,44`), `diff_apply` ≤ ~53 dispatches = 1 `repo.branchCreate` + ≤50 `repo.file.*` + 1 `pull.create` (performance-design.md:36; procs exist at `oxidean-api-repo/src/lib.rs:69,74–76`, `oxidean-api-pulls/src/lib.rs:22,31`).
- **Audit/monitoring surface** — the `audit_events` table exists (`oxidean-db/src/audit_events.rs`); the three `oxpilot.*` event names are new strings on the existing table, matching the gateway's `oxpilot_grant`/`oxpilot_action` precedent; the apply-failure leftover shape (`pending` row + `oxpilot/proposal-<id8>` branch, `apply_failed:<step>`) matches `reliability-design.md:113–137`.
- **Deployment narrative** — PR → `pr-<n>` → ephemeral → `sha-<sha>` → staging → manual promote matches `team.md` §Deployment; `images.yml` untouched is consistent with "same api image".
- **Traceability** — 78/78 coverage rows, all ids present in `nfr-design/traceability.json` upstream_ids (verified set-identical), all `OK`, targets point at real sections of the three artifacts.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
