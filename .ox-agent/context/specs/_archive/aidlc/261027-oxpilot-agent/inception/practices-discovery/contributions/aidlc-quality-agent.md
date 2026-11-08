**Collaborator:** aidlc-quality-agent

## Contribution

Independent verification of the lead draft's Testing Posture claims against
live evidence. Scope: coverage-floor composition, the `dialect_vcs` /
`web-browser` / `vcs-server` deltas, the `/oxpilot` manifest-row requirement,
and test-infrastructure gaps this intent will hit.

### Coverage floor — verified, draft accurate

- `scripts/coverage-weighted.sh` L7-10: weights `0.25/0.40/0.35`,
  `COVERAGE_WEIGHTED_FLOOR` default `0.70` — matches draft verbatim.
- `scripts/coverage-e2e-checklist.sh` L12-20: **exactly 7 ITEMS** — 2
  stack-browser (`auth-ui`, `forge-admin`), 2 stack (`smtp`, `oidc`), 3
  protocol smokes (`smoke-git-https`, `smoke-git-ssh`, `smoke-packages`).
  All 7 paths exist on disk (verified). Score = present/total; the draft's
  "presence checklist, not executed coverage" is literal — the script checks
  `[[ -f ]]`, never runs anything.
- `scripts/coverage-weighted.contract.sh`: self-test asserts under-floor
  rejects, over-floor and exact-0.70-boundary pass, and greps the weight
  constants — the gate is itself contract-tested (`make coverage-contract`
  runs first in the CI job, `ci.yml` L504-505).
- `ci.yml` `coverage-weighted` L506-530: Rust `cargo-llvm-cov` runs with an
  explicit comment "Collection only — the weighted gate composition
  (25/40/35) does not consume these numbers"; floor env set at L533;
  web summaries feed `--unit-json`/`--integration-json` + `--e2e-checklist`
  (L531-540). Draft's "consumes web numbers only, Rust collect-only" is
  correct — including that `oxidean-api-oxpilot` lands with no enforced
  coverage signal.

### `dialect_vcs` / `web-browser` / `vcs-server` — verified, but draft understates `db-dialects`

- `ci.yml` L886: the `-E` filter is **`dialect_probe` | `dialect_vcs` |
  `dialect_actions`** — three binaries, not the two the draft names.
  `dialect_actions` joined the filter in `c3c12c9a` (#158); `dialect_vcs`
  in `b4a2d405` (#159). `docs/TESTING.md` L309 still documents probe+vcs
  only — the lead appears to have re-derived from that stale table row.
- `db-dialects` additionally replays `binary_id(oxidean-api::workflow_
  conformance)` on real Postgres AND MySQL with per-test provisioned
  `oxt_<uuid>` databases via `OXIDEAN_TEST_DATABASE_URL` (ci.yml L894-907,
  `crates/oxidean-api/tests/support/mod.rs` L23-32 — unset falls back to
  sqlite pass-through, **not** skip). Cross-dialect CI execution is broader
  than the draft's "probe + vcs per dialect".
- **The "~30 `dialect_*` parity files `DATABASE_URL`-gated skip-pass" claim
  is materially wrong on both count and mechanism.** Reality: **23**
  `dialect_*` files in `crates/oxidean-db/tests/`; only **4** gate on
  `DATABASE_URL` (`dialect_probe`, `dialect_auth`, `dialect_vcs`,
  `dialect_actions` — verified by grep, e.g. `dialect_probe.rs` L11-21
  `eprintln!("skipping: DATABASE_URL unset"); return;`). The other ~19
  (e.g. `dialect_issues.rs` L44-46) are **sqlite-only migration/schema
  assertions that execute for real in the `api-rust` shards** — they are
  not parity tests and do not skip-pass.
- **`dialect_auth` gap (new finding):** added in `89e4a7c2` (#118), gated
  on `DATABASE_URL`, but **absent from the `db-dialects` `-E` filter** — it
  has never executed against Postgres/MySQL in CI and skip-passes
  everywhere. A gated suite that never runs is silent coverage loss; either
  it joins the filter or the exclusion needs documenting.
- `web-browser` job (ci.yml L384-405): 5 internal `vitest --shard=N/5`
  invocations, retry-once keyed strictly on `VITEST_BROWSER_CONNECTION_
  CLOSED` — draft accurate. Shard rationale is Chromium session lifetime
  (~8 files/session per the comment), not fault isolation.
- `vcs-server` (ci.yml L913-939): runs `-E 'package(oxidean-vcs)'` off the
  shared archive; load-bearing detail verified — `rust-build` archives
  `cargo nextest archive --workspace --features server` (L225-228), so the
  `#[cfg(feature = "server")]` suite exists in the archive and an empty
  match fails loud via `--no-tests`. `make test` mirrors the leg locally
  (`Makefile` L244,248).
- Flake-tolerance nuance: the draft's "shards re-run whole on failure —
  coarse flake tolerance" describes the shell-level `||` rerun (ci.yml
  L292-297). `.config/nextest.toml` `[profile.ci]` now also sets
  **`retries = 1`** (L19) — a bounded per-test retry covering lanes with no
  shell retry (coverage-weighted, db-dialects). The draft's flake story is
  right but incomplete.
- CI job count: **23 job stanzas** verified (incl. `e2e-stack`,
  `actions-e2e`, `build-metrics`, `rust-artifacts-cleanup`) — draft's "23"
  is right.

### `/oxpilot` manifest-row requirement — verified, and under-scoped in the draft

- `route-coverage.manifest.ts`: **82 rows** covering **82** `routes/*.tsrx`
  files (draft says ~83 — housekeeping), 2 `layoutOnly`, 3 `kind:"skip"`
  evidence entries; no `oxpilot` entries (route doesn't exist yet).
- `route-coverage-check.ts` L159-204: a new route with only `skip` evidence
  fails in change-aware mode — "newly added routes must have happy-dom or
  stack-browser proof (skip not allowed)". For `kind:"happy-dom"` the check
  verifies **file existence only** (`existsSync`, L131-137); the "must
  mount the page" requirement is doc-convention (TESTING.md L193), not
  enforced — worth knowing: a hollow `oxpilot.integration.test.ts` would
  technically pass inventory.
- **The coverage row is necessary but not sufficient for a new top-level
  route.** `scripts/check-route-sync.ts` (part of `make web-lint`, gated by
  `web-octane`) enforces a **three-way triangle**: `oxidean-web` `shells.rs`
  `SHELLS` dispatch + `apps/web/src/pages/**` Astro page emitting the dist
  shell + `route-params.ts` `RESERVED_TOP_SEGMENTS`. A literal top-level
  segment like `/oxpilot` omitted from `RESERVED_TOP_SEGMENTS` misbinds
  client-side as `/{owner}` — the draft's Testing Posture names only the
  coverage manifest.
- If the chat UI uses Select/Switch/portals/`form.Subscribe` (a model
  picker or dialog almost certainly does), `browser-coverage.manifest.ts`
  (57 surfaces today) needs a row with `subject` marker, and the
  `*.browser.test.tsx` is **source-verified**: mount
  (`mountComponent`/`mountWithQueryClient`), interact (`.click(` /
  `clickTestId` / `pickSelectOptionByTestId`), assert
  (`expectNoOctaneOverlayInDocument`/`expectNoDomRaces`/`trackDomErrors`)
  — `browser-coverage-check.ts` L35-108.
- `browser-coverage` job also runs `scripts/ui-coverage-change.contract.sh`
  (ci.yml L469) — a second step the draft doesn't enumerate.

### Domain-crate test conventions for `oxidean-api-oxpilot`

- `procedures_list_matches_dispatch` is a **per-crate convention**
  (`oxidean-api-core/src/rpc.rs` L335, `oxidean-api-pulls/src/dispatch.rs`
  L57, `oxidean-api-issues/src/lib.rs` L59) — the new crate needs its own
  copy; plus `rest_spec_procedures` scans the aggregated `PROCEDURES` and
  `check-api-domain-graph.mjs` enforces disjoint ownership — all fail on
  drift. `proc_classes.rs` L567: unlisted procs classify `SessionOnly`
  (fail-closed for PATs) — `oxpilot.*` needs no classification change to
  stay safe, and its tests enumerate names rather than iterate PROCEDURES,
  so no existing test breaks.
- `make test` = `nextest --workspace` + `nextest -p oxidean-vcs
  --features server` + `bun run test` (Makefile L241-250) — local/CI
  asymmetry claim verified (no browser/e2e/smokes locally).
- Zero-skip invariant re-verified: 0 `#[ignore]` under `crates/`, 0
  `it.skip`/`test.skip`/`describe.skip` under `apps/web`.
- Housekeeping drifts spotted: draft's "~83 rows" → 82; "(131 API test
  files)" → 130 under `crates/oxidean-api*/tests`; `web-lint` "five checks"
  → the target runs 6 commands (`Makefile` L411-417: bun lint + 5 check
  scripts); TESTING.md L309 job table stale (misses `dialect_actions` and
  the `workflow_conformance` leg).
- Precedent for provider-config UI: `settings/ai.tsrx` already carries a
  manifest row + happy-dom test — the natural home/shape for oxpilot
  provider settings.

## Positions

1. **Coverage-floor composition: affirm as drafted.** 0.25 unit / 0.40
   integration / 0.35 e2e-presence-checklist at floor 0.70 is exactly what
   the scripts, contract test, and CI env implement. My caveat: the e2e
   third is a **frozen 7-item denominator** — an `/oxpilot` stack test adds
   no score unless the checklist deliberately grows, and growing it is a
   gate change to be made consciously (each new item is a hard path
   dependency for every future PR). Interview should decide whether oxpilot
   e2e evidence joins the checklist or lands in job-gates only.

2. **`db-dialects` description must be corrected, not affirmed.** The draft
   repeats stale TESTING.md wording (probe+vcs) and the baseline's wrong
   "~30 gated skip-pass" framing. True state: 3 of 4 gated binaries run
   per-dialect + `workflow_conformance` on both real engines; `dialect_auth`
   is gated-but-never-run. Interview questions: (a) fold
   `dialect_auth` into the filter or document the exclusion; (b) fix the
   parity-gap wording — the debt is "only 4 files can even attempt
   non-sqlite dialects," not "30 files skip"; (c) if oxpilot adds
   `oxidean-db` tables, a `dialect_oxpilot.rs` must be added to the `-E`
   filter **and** the TESTING.md table in the same change or it's
   dead-on-arrival (no ruleset change needed — same job, filter arg only).

3. **The `/oxpilot` same-change registration set is bigger than one
   manifest row.** Interview/planning should enumerate the full quality-gate
   surface for the new route: `route-coverage` row with real happy-dom
   mount or stack-browser evidence (skip-only fails for added routes) +
   `check-route-sync` triangle (SHELLS + Astro page +
   `RESERVED_TOP_SEGMENTS`) + conditional `browser-coverage` row for
   high-risk controls + the generated `routeTree` sync. Recommend treating
   this as the unit-1 acceptance checklist.

4. **Collect-only Rust coverage: hold the affirmed stance, consciously.**
   Per change-type minimums, `oxpilot.*` RPC procedures land as "auth/RPC
   contract → Rust integration and/or stack e2e" — that is the enforced
   signal and it's adequate for this intent. But this is the first net-new
   domain crate since the #162 split to ship agentic/session state;
   interview should confirm we keep "no new numeric floor" rather than let
   this intent silently normalize crate-level coverage blindness. I do NOT
   propose a floor — I propose the interview records the decision.

5. **SSE streaming has no proven test-type precedent — the interview must
   assign a layer.** The job-log SSE pattern is affirmed as the *client*
   design, but nothing in the current suite proves stream correctness
   end-to-end: happy-dom can't exercise real SSE; `*.stack.test.ts` (node
   env, `OXIDEAN_E2E_API_ORIGIN`) can assert event-stream frames against
   the live API; stack-browser can prove the user-visible flow. Recommend:
   HTTP-level `.stack.test.ts` for the stream contract + happy-dom for
   append/fallback state + optional stack-browser click-through — and the
   interview should decide how much of this is unit-1 scope vs deferred.

6. **Computer-use connector = outbound HTTP → wiremock precedent applies.**
   Provider/connector calls should follow the existing wiremock fixture
   pattern for outbound HTTP (draft's tooling section) plus a
   trait+adapter seam; the QA-relevant point is that connector tests belong
   in the crate's `tests/` with mocked HTTP, not against live providers —
   and credentials-handling tests are a devsecops cross-over the interview
   should coordinate.

7. **No perf infrastructure — keep as recorded debt, but note the exposure
   shifts.** SSE streams, LLM round-trips, and computer-use sessions are
   latency/long-lived-connection surfaces the current suite can't measure.
   Affirm the debt; suggest NFR stage at minimum define smoke-level
   expectations (stream first-byte, session teardown) rather than inventing
   a harness.
