# Build and Test Summary — oxpilot

## Build status and prerequisites

Workspace builds via Cargo (`crates/*`) + Bun/Turborepo (`apps/*`, `packages/*`). Prerequisites: `bun install`, `.env` from `.env.example` with `OXIDEAN_OXPILOT_*` keys, DB URL per dialect, `make rpc-gen` after any RPC change. Heavyweight builds are CI-owned per the recorded user directive; the authoritative build signal is `ci.yml` run 40793221440 on `aa7ff9b3` (in progress at authoring). Locally-permitted gates (vitest, web-lint, web-format-check, `git diff --check`) all pass — see test-results.md.

## Test type inventory

- `build-instructions.md` — deps, env, build + verification commands, troubleshooting
- `integration-test-instructions.md` — Standard strategy: proc/dialect integration, web integration (73 tests), e2e stack, coverage/drift gates
- `performance-test-instructions.md` — NFR3.x latency + resource-bound targets (latency rows owned by operation-phase performance-validation)
- `security-test-instructions.md` — isolation/consent/redaction/budget checks + raw-sink audit
- `cross-unit-traceability.md` — Step-10 coverage gate: **PASS**, 122/122 enumerated ids
- `test-results.md` — executed commands + finalized Target Verification Matrix

## Coverage expectations per unit

Each of the 8 units ships its own test surface (see per-unit `unit-test-instructions.md`); deduplicated execution: nextest for `oxidean-api-oxpilot`, `oxidean-api --test oxpilot`, `oxidean-api-core`, `oxidean-db dialect_oxpilot`, `oxidean-cli`; vitest for the web surface; CI coverage job `coverage-weighted` guards regression.

## Target Verification Matrix

See `test-results.md` — 15 targets: **6 Met** (locally executed), **9 Unverified** (CI pipeline / performance-validation owned; in flight on `aa7ff9b3`, no in-stage or upstream defect identified). No `Pending`, no `Not Met`.

## Readiness assessment

- **Build-ready:** yes — commands documented; local gates green; CI executing the full matrix.
- **Test-ready:** yes — all instruction files generated; 73/73 local tests green.
- **Deployment-ready:** conditional on the in-flight CI run completing green; latency SLOs remain owned by operation-phase `performance-validation`.

## Known limitations

- Heavyweight Rust/Bun build+test execution is deferred to CI per the user's explicit directive — `Unverified` rows reflect ownership, not defects.
- The two p99 latency targets (BT-4, BT-5) require a deployed instrumented environment; operation-phase validation owns them.
- Single-replica caveat (in-memory active-turn state) is documented in the NFR design; no multi-replica claim is made.
