# Build and Test Summary — semver-release-automation

## Build status and prerequisites

Bun workspaces + Cargo workspace + generated artifacts (api-client,
openapi.yaml). Build chain documented in `build-instructions.md`;
`cargo check --workspace`, `bun install --frozen-lockfile`, `make
rpc-gen`, and `make version-check` all verified during code generation.
Docker daemon and `act` confirmed present for the compose/workflow
checks.

## Test type inventory

- Unit/integration per unit (U1–U7): Bun suite `scripts/versioning`,
  Rust crate tests, api-client Vitest, docs-shape guard — all
  re-executed once in this stage.
- Cross-unit integration: `integration-test-instructions.md` (this stage,
  Standard strategy).
- Performance: N/A (no performance NFRs).
- Security: N/A dedicated battery; NFR5/RELEASE_TOKEN/header-rejection
  properties asserted by existing suites.

## Coverage expectations per unit

| Unit | Suite | Expectation |
|---|---|---|
| u1 | registry/check tests inside `bun test scripts/versioning` | green |
| u2 | commit grammar tests inside same suite | green |
| u3 | plan engine + temp-repo bump integration inside same suite | green |
| u4 | changelog + release-tags + workflow-shape inside same suite | green |
| u5 | hook e2e + CI-shape inside same suite | green |
| u6 | Rust core/api/cli + api-client Vitest + web lint/format | green |
| u7 | docs-shape tests inside same suite | green |

## Target Verification Matrix

| Target ID | Source | Expected | Actual | Evidence | Owning Stage | Verdict |
|---|---|---|---|---|---|---|
| T1 | U1/U5/U7 | `version-check` → 10 components ok | 10 components, 1 baseline warning | `make version-check` | build-and-test | Met |
| T2 | U1–U5/U7 | `bun test scripts/versioning` ≥126 | 126 pass / 0 fail | bun output | build-and-test | Met |
| T3 | U3 | `commit-check` HEAD range ok | all ok; advisory warnings only | make output | build-and-test | Met |
| T4 | U6 | `cargo test -p oxidean-core` green | 55 pass | cargo output | build-and-test | Met |
| T5 | U6 | `cargo test -p oxidean-cli` manifest green | 8 pass | cargo output | build-and-test | Met |
| T6 | U6 | `cargo test -p oxidean-api` cli_dist+rpc green | 6+9 pass | cargo output | build-and-test | Met |
| T7 | U6 | api-client Vitest ≥6 | 6 pass | vitest output | build-and-test | Met |
| T8 | U6 | client regen drift-free | regen byte-identical; sync gate is post-commit | sha256 ×2 match | build-and-test | Met |
| T9 | U6 | `openapi-gen` deterministic + `0.1.0` | sha256-stable; `info.version: 0.1.0` | regen ×2 + grep | build-and-test | Met |
| T10 | U6 | web-lint + format clean | both clean | make output | build-and-test | Met |
| T11 | U4/U5 + posture | YAML + embedded bash clean | 6 YAML ok; 111 bash blocks ok; 3 pwsh excluded | python extractor | build-and-test | Met |
| T12 | Posture (docker) | `compose config -q` exit 0 | exit 0 | docker output | build-and-test | Met |
| T13 | Posture (act) | `act -l` parses touched workflows | 5/5 parse; `commit-lint` listed | act output | build-and-test | Met |
| T14 | NFR4/NFR5 | hook+CI gate; no silent mutation; no secrets | suite + workflow assertions hold | hook/docs tests | build-and-test | Met |
| T15 | FR1–FR8 | every requirement covered `OK` | 50/50 covered; cross-unit-traceability.md PASS | that file | build-and-test | Met |

## Readiness assessment

- Build-ready: yes — `cargo check --workspace` clean; regen deterministic.
- Test-ready: yes — ~760 assertions green across Bun/Rust/Vitest suites.
- Deployment-ready: yes for merge — release lanes statically validated
  (YAML + `act` job graphs); live tag-push execution is the CI Pipeline
  stage's concern and needs `RELEASE_TOKEN` post-merge.

## Known limitations

- Release workflows are validated via YAML parse + `act -l` job graphs,
  not full execution — real runs need `RELEASE_TOKEN` and a pushed tag.
- `docs/openapi.yaml` diff vs HEAD is expected pre-commit (regenerated
  in U6); the sync gate is meaningful post-commit.
- Pre-existing `GIT_TEMPLATE_DIR` flake in `oxidean-api` `repo_fork`
  tests under parallel `cargo test`; nextest or `--test-threads=1`
  avoids it — unrelated to this scope.

## Sources

Consumes per unit `code-generation-plan.md`, `unit-test-instructions.md`,
and `code-summary.md` under `construction/*/code-generation/` (all seven
units), plus the stage-level Testing Contract they embed.

