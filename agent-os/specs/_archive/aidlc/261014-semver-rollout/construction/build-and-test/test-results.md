# Test Results — semver-release-automation (Build and Test)

## Build status

`cargo check --workspace` — **success** (0.22s incremental, no warnings).
`bun install --frozen-lockfile` — previously satisfied. Generators
re-verified deterministic: `make rpc-gen` and `make openapi-gen` produce
byte-identical output on re-run (sha256 match, NFR2).

## Test results (all executed once, deduplicated)

| Command | Result |
|---|---|
| `make version-check` | ok — 10 components registered, 1 pending-baseline warning (expected until first release commit) |
| `bun test scripts/versioning` | **126 pass / 0 fail** (13 files; incl. 11 new docs-shape) |
| `make commit-check` (HEAD~10..HEAD) | all commits `ok`; unmapped-scope warnings advisory only |
| `cargo test -p oxidean-core` | 55 pass / 0 fail |
| `cargo test -p oxidean-cli --lib manifest` | 8 pass / 0 fail |
| `cargo test -p oxidean-api --test cli_dist` | 6 pass / 0 fail |
| `cargo test -p oxidean-api --lib rpc` | 9 pass / 0 fail |
| `bunx vitest run` (packages/api-client) | 6 pass / 0 fail |
| `bun run test` (turbo: api-client + web) | api-client 6 + web **548 pass** / 0 fail (125 files) |
| `make web-lint` | clean (oxlint + DOM-race + anchors + route-sync 75↔76) |
| `make web-format-check` | clean (475 files) |
| `make openapi-gen` ×2 | byte-identical; `info.version: 0.1.0` |
| Workflow YAML parse (6 files) | all valid |
| Embedded `run:` bash `-n` | 111 bash blocks clean, 3 pwsh steps correctly excluded, 0 failures |
| `docker compose -f docker-compose.yml config -q` | exit 0 |
| `act -l` × 5 workflows (ci, release, release-pr, ox-cli, publish-runner) | all job graphs parse; `commit-lint` job present in ci.yml |

**Totals: ~760 test assertions green, 0 failures, 0 skipped.**

## Failure details

None. The only non-zero exit observed was `make rpc-sync-check` — expected:
the gate is `make rpc-gen` + `git diff --exit-code` on `packages/api-client`,
i.e. a committed-tree sync check that must fail while the regenerated client
is intentionally uncommitted. Deterministic regen (sha256-stable) stands in
for it locally; the gate becomes authoritative post-commit in CI.

## Coverage report

No line-coverage floor applies to this scope (standard strategy;
scope floor = keep existing suite green — satisfied).

## Finalized Target Verification Matrix

| Target ID | Source | Expected | Actual | Evidence | Owning Stage | Verdict |
|---|---|---|---|---|---|---|
| T1 | U1/U5/U7 | `version-check` → 10 components ok | 10 components, 1 baseline warning | `make version-check` | build-and-test | Met |
| T2 | U1–U5/U7 | `bun test scripts/versioning` ≥126 | 126 pass / 0 fail | bun output | build-and-test | Met |
| T3 | U3 | `commit-check` HEAD range ok | all ok; advisory warnings only | make output | build-and-test | Met |
| T4 | U6 | `cargo test -p oxidean-core` green | 55 pass | cargo output | build-and-test | Met |
| T5 | U6 | `cargo test -p oxidean-cli` manifest green | 8 pass | cargo output | build-and-test | Met |
| T6 | U6 | `cargo test -p oxidean-api` cli_dist+rpc green | 6+9 pass | cargo output | build-and-test | Met |
| T7 | U6 | api-client Vitest ≥6 | 6 pass | vitest output | build-and-test | Met |
| T8 | U6 | client regen drift-free (post-commit gate) | regen byte-identical; git-diff gate N/A pre-commit | sha256 ×2 match | build-and-test | Met |
| T9 | U6 | `openapi-gen` deterministic + `0.1.0` | sha256-stable; `info.version: 0.1.0` | regen ×2 + grep | build-and-test | Met |
| T10 | U6 | web-lint + format clean | both clean | make output | build-and-test | Met |
| T11 | U4/U5 + posture | YAML + embedded bash clean | 6 YAML ok; 111 bash blocks ok; 3 pwsh excluded | python extractor | build-and-test | Met |
| T12 | Posture (docker) | `compose config -q` exit 0 | exit 0 | docker output | build-and-test | Met |
| T13 | Posture (act) | `act -l` parses touched workflows | 5/5 parse; `commit-lint` job listed | act output | build-and-test | Met |
| T14 | NFR4/NFR5 | hook+CI gate; no silent mutation; no secrets | hook.test.ts+docs.test.ts in suite; `make setup` explicit; RELEASE_TOKEN gating asserted | suite + workflows | build-and-test | Met |
| T15 | FR1–FR8 | every requirement covered `OK` | all covered; see cross-unit-traceability.md | that file | build-and-test | Met |

## Deferred / noted

- Full-workspace `cargo nextest run --workspace --profile ci` is owned by
  the CI Pipeline stage (api-rust job, ~1650s suite) — not re-run here;
  every crate this scope touched was tested directly.
- Real release-lane execution (tag push → release assets) requires a merge
  + `RELEASE_TOKEN`; statically validated here.

## Sources

Consumes per unit `code-generation-plan.md`, `unit-test-instructions.md`,
and `code-summary.md` under `construction/*/code-generation/` (all seven
units), plus the stage-level Testing Contract they embed.

