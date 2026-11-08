### Review — infrastructure-design / connector-client

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
| R-01 | Major | `cicd-pipeline.md` §Lane coverage, `check-cli-parity.sh` row + preamble claim | The doc presents the parity check as a firing lane ("Every lane that must cover U7 already exists and already fires on its paths"; row evidence column "evidence it fires"; "an `implemented` row … fails the check"). **Nothing invokes `check-cli-parity.sh` in CI.** Zero matches for `parity`/`cli-parity` anywhere under `.github/workflows/`; the `rpc-sync` job runs rpc/openapi/skill/workflow-template/stack-preset/api-domain-graph checks only; `scripts/check-rpc-sync.sh` diffs only `packages/api-client`; no `.githooks` or aggregate Make target calls it — `Makefile` defines `cli-parity-check` but nothing runs it. A PR shipping `implemented` `pilot` rows without a parseable `ox pilot` group merges green in every CI lane. | Pick one and say so: (a) wire it in — the `rpc-sync` job already downloads `rust-bins` (which includes `target/debug/ox`) and sets `OXIDEAN_PREBUILT_BINS=1`; `OX_BIN=target/debug/ox ./scripts/check-cli-parity.sh` is a one-step addition inside an existing job, so "no new jobs / no ruleset edit" still holds; or (b) reword the row to state plainly the parity check is a manual/local gate (`make cli-parity-check`, never run by CI) and the same-change rows are a docs-honesty registration, not CI-enforced drift coverage. | Open |
| R-02 | Minor | `cicd-pipeline.md` §Lane coverage, same row | "an `implemented` row whose **verb** doesn't parse fails the check" misstates the probe's granularity. The awk (`check-cli-parity.sh`) splits on `\|`, requires `$4 ~ /implemented/`, and extracts only the *first* backticked `[a-z-]+` token — `` `pilot pair` `` extracts `pilot`; all four prescribed rows collapse to one `ox pilot --help` probe via `sort -u`. Failure is only detected when the output contains "unknown command", i.e. group-level. A `pilot frobnicate` row marked `implemented` passes while the group parses. | Reword to group-level accuracy ("an `implemented` row whose *group* doesn't parse fails the check — the probe is `ox pilot --help`; per-verb registration is docs completeness, not gate-verified"). | Open |
| R-03 | Minor | `infrastructure-design/traceability.json` coverage rows NFR5, NFR7, NFR8 | Three N/A notes misname the upstream families against `inception/requirements-analysis/requirements.md`: NFR5 is **Auditability** ("connector actions, approvals, and target lifecycle emit `audit_events`") — the note calls it the "observability family carried by NFR6.x" (the honest rationale is server-owned emission, no client footprint — the client emits no audit per NFR6's scope boundary); NFR7 is **Accessibility/WCAG AA** — the note calls it "usability family … carried by NFR6.4/6.6" (disclosure pins do not cover WCAG; the correct rationale is no UI surface in a headless CLI); NFR8 is **Usability** ("disabled/unconfigured state is explanatory…") — the note calls it "i18n/accessibility family". The N/A verdicts themselves are defensible; the labels are wrong. | Correct the three `target` notes: NFR5 → "server-owned audit emission (U6 `oxpilot.*` audit rows) — no client audit footprint"; NFR7 → "WCAG AA — N/A, no web UI surface in a CLI unit"; NFR8 → "usability — carried by NFR6.4/6.6 disclosure + operator-actionable error text". | Open |
| R-04 | Minor | `cicd-pipeline.md` §Docs surfaces table | The same-change registration list omits `docs/CLI.md`: its `## Commands` section enumerates *every* `ox` group exhaustively — a shipped `pilot` group absent there is a user-visible reference gap against the doc's own "the contract must be readable without source" rationale. `crates/oxidean-cli/skill/SKILL.md` (cheat-sheet table) is likewise unaddressed — adding `pilot` mentions there is optional, but if touched it is CI-gated by `skill-sync-check` (rpc-sync job; `check-skill-sync.sh` validates every `ox …` mention + flag against `ox completion commands`/`--help`), which deserves the same awareness the CLI-PARITY row gets. | Add a `docs/CLI.md` row to the table (Commands section gains the `pilot` verbs), and record the SKILL.md decision (either "add a pilot row to the cheat sheet — `skill-sync-check` re-verifies every mention in the rpc-sync job" or "deliberately excluded — connector lifecycle is operator-run, not agent-driven"). | Open |

### Verified claims (no defect)

For the conductor's record — the rest of the primary target's claims all check out:

- **Archive claim accurate**: `ci.yml` runs `cargo nextest archive --workspace --features server` verbatim; `api-rust` shards replay it with `--partition count:N/5` and **no `-E`** — a `crates/oxidean-cli/tests/pilot_connector.rs` binary partitions in automatically (existing `manifest_gate.rs`/`rpc_http.rs`/`self_update_http.rs` already run there). The contrast is also right: `db-dialects` is filterset-driven (`-E 'binary_id(oxidean-db::dialect_oxpilot)'`); `vcs-server` filters `package(oxidean-vcs)`.
- **Matrix count exact**: `ox-cli.yml` has exactly 9 `include` legs (gnu ×3, musl ×2, macOS ×2, Windows ×2 incl. `windows-11-arm`) + the `build-freebsd` VM job. `paths:` includes `crates/oxidean-cli/**` for **both** `push` and `pull_request`. Smoke scripts `scripts/ox-smoke.{sh,ps1}` exist and are wired.
- **`cargo build --workspace --bins`** builds `ox` (`Cargo.toml` `[[bin]]`; uploaded in `rust-bins` with `if-no-files-found: error`).
- **`wiremock = "0.6.5"`** is a `[dev-dependencies]` entry, alongside `tempfile` for the custody tests. `base64 = "0.23"` is the existing workspace pin (5 crates); `secrecy`/`zeroize` genuinely absent — "new deps" correct. `cargo install --locked` used in `docker/ci-base/Dockerfile`; dependabot weekly cargo batch confirmed.
- **Registries real**: `args.rs` has `USAGE` (top-level help asserted by the smokes' `ox <command>` grep), the `HELP` table, the `GROUPS` const (the "unknown command" gate), `Command` enum + dispatch match, `required_procedures` (non-RPC `pilot` arms must return `vec![]` — compiler-forced by the exhaustive match; an `oxpilot.*` literal there would fail `skill-sync-check` since connector routes aren't RPC procs), and `completion::COMMAND_TREE` (args.rs test fail-closed on missing tree entries). Registration points named in the spec ("Command enum + usage-registry entries + dispatch arms") are adequate since every match site is compile/test-enforced.
- **Workspace-wide gates**: `cargo fmt --all`, `cargo machete` + `clippy --workspace --all-targets -D warnings` + `cargo doc --workspace`, commit-lint on PR title+range. `feat(cli)` is a registered scope (`versioning.toml` `ox` component, scopes `["cli","ox"]`); `feat(oxidean-cli)` is unmapped-but-valid (warning + glob attribution to `ox`).
- **nextest ci profile** = `retries = 1`, `slow-timeout 90s` — cited correctly.
- **openapi.yaml not drift-gated**: `openapi-gen --check` regenerates and diffs only `docs/openapi.yaml`; `crates/oxidean-cli/src/pilot/openapi.yaml` trips no scan.
- **U6 server surface exists**: `liveness_fresh` 90s window, `last_seen_at` column (migration `0088_oxpilot_connector.sql` ×3), `oxpilot.target_connected/_disconnected/_revoked` audit events. Monitoring's "no metrics" matches the pinned scope boundary ("No metrics endpoint, no tracing export, no audit emission").
- **Constants consistent**: 64 KiB tail / 128 KiB `FS_IO_CAP` / 256 KiB `OUTPUT_MAX` / 300s `EXEC_TIMEOUT` match `nfr-design/performance-design.md` and `reliability-design.md`; 15s heartbeat inside 90s window matches `BR8.5` + shipped `HEARTBEAT_LIVENESS_WINDOW_SECS`; exit-code contract matches `CliError` `EXIT_USAGE=2`/`EXIT_AUTH=3` with the documented divergence rationale intact.
- **Env-var claim holds**: custody rides the existing `keystore::{set,get,delete}` seam + `config_dir()` (existing `OXIDEAN_CONFIG_DIR`); `.env.example`/`docs/CONFIGURATION.md` untouched is consistent with zero new `OXIDEAN_*` vars.
- **"No new jobs / no ruleset edit" true as written** — no workflow file changes are proposed; nothing is added/renamed/removed, so the merge-gate list is untouched.

### Notes for iteration 2

- The cheapest fix for R-01 that preserves the doc's "no workflow-file change" headline is option (b) — reword the row as a manual gate. Option (a) is also legitimate (one step inside `rpc-sync`, `OX_BIN=target/debug/ox`) and would *strengthen* the design; either resolves the finding.
- R-02 is same-location wording; likely resolved in the same rewrite as R-01.
- R-03/R-04 are isolated corrections in `traceability.json` and the Docs surfaces table.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
