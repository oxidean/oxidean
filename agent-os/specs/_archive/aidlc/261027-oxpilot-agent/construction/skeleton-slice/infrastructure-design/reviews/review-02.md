### Infrastructure Design Review — u1-skeleton-slice

### Review

**Verdict:** READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 2

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | major | cicd-pipeline.md > build & test table `job: e2e-stack` row (l.27) + Secrets & config section (l.63-65) | Enabled-path stack probe could not self-provide enablement: `*.stack.test.ts` runs against the API spawned by `scripts/dev-auth/run-stack-e2e.sh`'s fixed export set; `OXIDEAN_OXPILOT_ENABLED` defaults OFF (BR2.2). | Add harness env exports + `e2e/stack/` landing to the registration set; correct the mechanism claim. | Resolved |
| R-02 | minor | cicd-pipeline.md > build & test table (l.23-25) | One `coverage-weighted` row conflated three distinct jobs (route-coverage, browser-coverage, coverage-weighted). | Split into per-job rows naming each enforcing gate. | Resolved |
| R-03 | minor | cicd-pipeline.md > `job: route-coverage` row (l.23) + `job: browser-coverage` row (l.24) | Manifest registration understated: BR5.1 requires rows for BOTH `/oxpilot` and `/oxpilot/$session`; browser row was conditional. | Name both route rows; state the browser-coverage row is expected for the session form's controls. | Resolved |
| R-04 | minor | cicd-pipeline.md > `job: commit-lint` row (l.29) | Registration omitted the reverse `depends_on` edge — oxpilot-only changes would stop rolling the api binary's component version while CI passes silently. | Record adding `oxidean-api-oxpilot` to `oxidean-api.depends_on`. | Resolved |
| R-05 | minor | infrastructure-specification.md > Deployment > Networking topology (l.18) | Compression-exclusion mechanism miscited: exclusion lives on the web-proxy path; api has no compression; gateway gzip has no carve-out. | Re-cite: web-proxy exclusion, no api compression layer, shipped SSE precedent across gateway gzip. | Resolved |
| R-06 | minor | traceability.json > NFR4.3 (l.231-234) | Target cited cicd-pipeline.md, which contains no orphan-sweep content. | Re-point to monitoring-design.md (or mark N/A). | Resolved |
| R-07 | minor | cicd-pipeline.md > build & test table surface column (l.14-16, 26) | Column mixed ci.yml job names with make-target/script names; `web-format`/`check-route-sync` were ungreppable as jobs. | Annotate job vs make/script entries so the set greps cleanly. | Resolved |
| R-08 | minor | infrastructure-specification.md > Deployment > IaC approach (l.21) | Dashboard-set `OXIDEAN_OXPILOT_*` vars could be pruned by a later declarative `railway config apply`, silently reverting enablement. | Record durability or declare envs via `preserve()`. | Resolved |

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|

None.

### Validation notes

- **R-01 verified against repo:** `e2e-stack` job exists at `.github/workflows/ci.yml:558-559` and runs `make test-e2e-stack` (l.598) → `scripts/dev-auth/run-stack-e2e.sh` (Makefile:225-226), which exports a fixed env set (l.125-157) inherited by the spawned `oxidean-api`. Adding the two exports there is the correct chokepoint (serves local + CI); `apps/web/vitest.config.ts:175` globs `e2e/stack/**/*.stack.test.ts` into the `e2e-stack` project under `E2E_STACK=1` — the landing directory exists with the same file pattern. Upstream confirms `PROVIDER=stub` ⇒ `StubProvider` and `OXIDEAN_OXPILOT_MODEL` defaults `stub-1` (rules.md BR2.5), so the two named exports suffice — no `_MODEL` export needed.
- **R-02/R-03 verified:** three rows now name their jobs; both route names match functional-spec.md l.116 (`/oxpilot`, `/oxpilot/$session`, BR5.1); browser row states "expected" with the manifest's control classes (`Select*`/portal/`form.Subscribe`) — the session form's repo binding (`session_create {repo_owner, repo_name, …}`) makes a high-risk control the expected outcome.
- **R-04 verified:** `versioning.toml:71` `oxidean-api.depends_on` enumerates all `oxidean-api-*` domain crates exactly as the row describes; the new crate joins `[[component]]` + CHANGELOG + the reverse edge.
- **R-05 verified:** new networking-row wording matches the repo facts confirmed in pass 1 (`crates/oxidean-web/src/main.rs:122` exclusion on the proxy path only; no api compression layer; Caddy `encode gzip` global with the shipped `stream_job_log` precedent traversing it).
- **R-06 verified:** NFR4.3 target now reads `monitoring-design.md — orphan-sweep-count startup signal (sweep itself is pre-accept api boot code, reliability-design.md)` — monitoring-design.md l.20 carries the signal; reliability-design.md l.15 carries the synchronous pre-accept boot step. Both citations exist.
- **R-07 verified:** legend sentence added (l.14-16); every job named `job:` matches ci.yml (rust-build, api-rust, db-dialects, route-coverage, browser-coverage, coverage-weighted, e2e-stack, smoke-protocol, actions-e2e, commit-lint, web-octane). Residual nit, not a finding: the row writes `bun scripts/check-route-sync.ts` while Makefile:416 literally reads `bun run scripts/check-route-sync.ts` — functionally identical and the script name still greps cleanly.
- **R-08 verified:** `preserve()` exists in the vendored railway IaC SDK (`railway/dist/index-C3uk0ruc.d.ts:403`) and is the established pattern in `.railway/railway.ts` (l.23, 58, 87-109…) for dashboard-owned vars. The IaC row's reasoning (declarative apply prunes undeclared vars; preserve() keeps values while absence stays flag-off-valid) matches the SDK semantics and NFR posture. No CI job validates/applies railway.ts (`cloud-plan` is review-only, apply is human-approved), so its absence from the pipeline gate table is correct — infra-spec is the right locus. The cicd rollout text no longer claims railway.ts is untouched.
- **Re-verified unchanged claims:** 53/53 NFR coverage rows, valid JSON, statuses defensible; `docs/TESTING.md:309` does document the `-E` filter in its job table (making the "and TESTING.md job table" registration accurate); TESTING.md's stale omission of `dialect_actions` remains pre-existing drift, not U1's; SLI ceilings (250/150 ms, N=1) match performance-requirements.md NFR3.1-3.5; `oxpilot_session.create` audit naming matches NFR5.1/observability-design.md:32; StubProvider constructor-only flag matches reliability-design.md (NFR4.10).
- **Nits considered and dismissed:** (a) `e2e-stack` gate column's "no other mechanism can reach server env" is rhetorically strong — a job-level `env:` would also propagate — but the harness export is the correct single registration point and the sentence's operative claim (the probe cannot self-enable) is true; (b) NFR3.16's target bundles "cascade FK + growth bound" under the storage row while the 64 KiB × one-turn-per-session bound lives in sizing — same doc, combined coverage present, accepted in pass 1.

### Summary

All eight iteration-1 findings landed correctly and were verified against the repo, not taken at face value. The R-01 blocker is genuinely fixed: the `e2e-stack` row now registers the `run-stack-e2e.sh` harness exports (`OXIDEAN_OXPILOT_ENABLED=1` + `OXIDEAN_OXPILOT_PROVIDER=stub`) — verified as the only viable enable path for the spawned server, with `_MODEL` correctly omitted (defaults `stub-1`) — and names the `apps/web/e2e/stack/` landing that the `e2e-stack` vitest project actually globs. The secrets section's mechanism claim was corrected to match. The three coverage jobs are split with both route rows named and the browser row marked expected; the `oxidean-api.depends_on` reverse edge is recorded; the compression mechanism is re-cited to the correct loci (web-proxy exclusion / no api compression / gateway gzip precedent); NFR4.3 re-points to monitoring-design.md with an accurate reliability-design.md secondary citation; the surface column's `job:` convention makes every entry greppable; and the IaC row's `preserve()` declarations match the vendored railway SDK's actual semantics with no CI gate to register. Revision introduced no new defects; remaining notes are sub-finding nits only. The registration set is now complete and internally consistent across all four artifacts.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
