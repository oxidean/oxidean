# Integration Test Instructions — semver-release-automation

Standard strategy: per-unit suites already ran green during code
generation. This stage re-runs the union once (deduplicated), plus the
cross-unit boundaries the units exercise together: registry → grammar →
plan → changelog → tags, hook → commit-check → CI job, wire version →
server/CLI/generated-client surfaces, docs ↔ registry consistency.

## Framework / runner

- Bun test: `bun test <path>` (scripts/versioning suite)
- cargo-nextest `0.9.143` (workspace; falls back to `cargo test`)
- Vitest via turbo: `bun run test` (web + api-client)
- Shell checks: `bash -n`, python `yaml.safe_load`, `docker compose config`, `act -n`

## Commands (each run ONCE — covers the union of per-unit instructions)

```bash
# U1/U5/U7 — registry, hook, docs-shape
make version-check                                  # 10 components, registry consistent
bun test scripts/versioning                         # registry/commit/plan/changelog/hook/docs/workflow suites

# U2/U3 — grammar + plan engine end-to-end on real repo history
make commit-check                                   # lints HEAD~10..HEAD against the grammar
bun scripts/versioning/cli.ts plan --dry-run 2>/dev/null || bun scripts/versioning/cli.ts plan --channel stable --json | head -5

# U6 — wire surfaces
cargo test -p oxidean-core                          # wire_compatible + manifest + MCP pin
cargo test -p oxidean-cli --lib manifest            # BR6.4 fallback paths
cargo test -p oxidean-api --test cli_dist           # /cli/latest latest.json cases
cargo test -p oxidean-api --lib rpc                 # dual-accept header matrix
bunx vitest run --root packages/api-client          # generated-client surface (or bun run test)

# U5 — hook e2e (inside scripts/versioning suite above) + CI shape
python3 -c "import yaml,glob; [yaml.safe_load(open(f)) for f in glob.glob('.github/workflows/*.yml')]; print('yaml ok')"

# U4 — embedded workflow bash syntax (extract run blocks → bash -n)
# (see command in test-results.md — python extractor + bash -n per block)

# Cross-cutting
cargo check --workspace                             # all crates compile incl. new dep edges
make web-lint                                       # oxlint + dom-race + anchors + route-sync
make web-format-check                               # oxfmt
make rpc-sync-check                                 # generated client drift gate
make openapi-gen && git diff --quiet -- docs/openapi.yaml || echo "openapi regen produced diff (expected pre-commit; verify content instead)"
docker compose -f docker-compose.yml config -q      # compose syntax (daemon not required)
act -l -W .github/workflows/release.yml             # workflow parse/job-graph validation
act -l -W .github/workflows/release-pr.yml
act -l -W .github/workflows/ox-cli.yml
act -l -W .github/workflows/publish-runner.yml
act -l -W .github/workflows/ci.yml
```

## Boundaries exercised

| Boundary | Evidence command |
|---|---|
| Registry → grammar → detect → plan → changelog → tags | `bun test scripts/versioning` (temp-repo integration tests in `commit/plan/changelog` suites) |
| Hook → local lint → CI gate | `hook.test.ts` (in suite) + `make commit-check` + ci.yml `commit-lint` YAML |
| `WIRE_VERSION` → server gate → CLI header → generated client → OpenAPI | `cargo test` trio + `vitest run` + `make rpc-sync-check` + `make openapi-gen` diff |
| `latest.json` → `/cli/latest` → self-update honesty | `cli_dist` integration tests |
| Docs ↔ registry (BR7.3 no-duplication) | `docs.test.ts` (in suite) |
| Release workflows → tag lanes → `latest.json` | YAML parse + `act -l` job graphs |

## Coverage expectations

- `bun test scripts/versioning` ≥ 115 pass (baseline) + 11 docs-shape = 126.
- Rust touched-crate tests: core/cli/api lib + cli_dist integration all green.
- api-client vitest: 6 pass.
- YAML parse: 6 workflows clean; embedded bash: all extracted blocks `bash -n` clean.
- `act -l`: every touched/new workflow lists jobs without parse errors.
- `docker compose config -q`: exit 0.
- openapi regen: deterministic; content carries `info.version: 0.1.0`.

## Test data / environment

- Temp-git fixtures are created inside the suites themselves (no seeding).
- `act` and `docker` need no daemon for the commands above (`-l`/`config`
  are static); `docker info` live-verified for any future smoke lanes.
- The `GIT_TEMPLATE_DIR` parallel flake in `oxidean-api` `repo_fork` tests
  is pre-existing; `--test-threads=1` or nextest process isolation avoids it.

## Sources

Consumes per unit `code-generation-plan.md`, `unit-test-instructions.md`,
and `code-summary.md` under `construction/*/code-generation/` (all seven
units), plus the stage-level Testing Contract they embed.

