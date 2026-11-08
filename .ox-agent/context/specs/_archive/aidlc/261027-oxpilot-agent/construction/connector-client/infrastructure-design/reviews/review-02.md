### Review — infrastructure-design / connector-client

**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 2
**Verdict:** READY

### Review

### Prior findings

| ID | Prior finding | Resolution |
|----|---------------|------------|
| R-01 | Parity check presented as firing CI lane; nothing invokes `check-cli-parity.sh` | **Resolved** — row now states the check has no CI caller today (verified: zero refs under `.github/`; Makefile target is the only caller) and prescribes `OX_BIN=target/debug/ox ./scripts/check-cli-parity.sh` as a new step inside `rpc-sync` (verified: job env `OXIDEAN_PREBUILT_BINS=1`; downloads `rust-bins` → `target/debug`; `ox` is in the artifact). Preamble corrected to "One workflow-file change". |
| R-02 | Probe granularity misstated as verb-level | **Resolved** — row now describes the group-level probe (`ox pilot --help`; first backticked token; `sort -u` collapse; "per-verb registration is docs completeness, not gate-verified") — exact match to `check-cli-parity.sh`. |
| R-03 | NFR5/NFR7/NFR8 target notes misnamed the upstream families | **Resolved** — NFR5→server-owned audit emission/no client footprint; NFR7→WCAG, no UI surface; NFR8→usability carried by NFR6.4/6.6. |
| R-04 | `docs/CLI.md` and SKILL.md absent from docs table | **Resolved** — `docs/CLI.md` Commands row added; SKILL.md consciously excluded with the `skill-sync-check` rationale (mechanics verified against `check-skill-sync.sh`). |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-05 | Minor (non-blocking) | `cicd-pipeline.md` §Docs surfaces, `docs/CLI-PARITY.md` row | The prescribed `pilot` rows are "marked `implemented` (ox side; gh column `—`)" — the first row in the file to combine gh `—` with `implemented`; every existing gh-`—` ox surface uses `ox-only` (the legend's exact fit for pilot). The choice is *correct* — the awk only probes `$4 ~ /implemented/`, so `ox-only` would escape the wired gate — but the doc never says the convention alternative was rejected. A future gh-pin reconciliation could "fix" the rows to `ox-only` and silently drop the only CI coverage. | One clause in the row: "`implemented`, not `ox-only` — the awk probes only `implemented` rows, so `ox-only` would exempt `pilot` from the gate." Tolerable as-is at codegen (the instruction is unambiguous). | Open — nit |

**Notes for the record (verified, no defect):** the wired step cannot hit the toolchain-less `cargo build` fallback (`OX_BIN` set + artifact always contains `ox` with `if-no-files-found: error` + `chmod +x` — same pattern `check-skill-sync.sh` already uses in this job); `gh`-pin extraction still resolves via the `| gh 2.96.0 |` table header; rpc-sync runs in the tools container under pinned `shell: bash`; no path filters on `pull_request` so the gate fires on the U7 PR; forward/reverse traceability is consistent (63/63 upstream ids covered; reverse targets match forward rows).

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
