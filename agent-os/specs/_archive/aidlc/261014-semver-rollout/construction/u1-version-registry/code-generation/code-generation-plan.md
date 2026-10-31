# U1 Version Registry — Code Generation Plan

Implements the `versioning.toml` registry, its loader/validator, the `check`
subcommand, and the one-time manifest migration to literal `0.1.0` versions.
Methodology: test-after (per Testing Contract) — implement each layer, then
write and run that layer's tests.

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "org",
  "ordering": "implement each applicable testable layer, then write and run that layer's tests.",
  "scope": "semver-release-automation",
  "test_strategy": "standard",
  "project_type": "brownfield",
  "applicable_notes": [
    {
      "layer": "org",
      "text": "We treat tests as a first-class deliverable in every Bolt. The specific\nmethodology (TDD, BDD, ATDD, or classic test-after) is affirmed at\npractices-discovery and recorded in `team.md` under this heading with explicit\n`Methodology` and `Ordering` fields; Code Generation resolves those fields\nindependently from coverage, tooling, and scope notes.\n\nWhen no posture has been affirmed, our default per scope is:\n- **Methodology**: test-after\n- **Ordering**: implement each applicable testable layer, then write and run\n  that layer's tests.\n- `mvp`, `enterprise`, `feature`, `infra`, `classic` add an 80% line-coverage\n  floor and CI execution before merge.\n- `bugfix`, `security-patch` add a targeted regression for the specific\n  bug/vulnerability and require the existing suite to remain green.\n- `express` uses the Minimal strategy: requirement-driven unit tests (one per\n  requirement, with a happy-path floor per component); existing tests remain\n  green.\n- `poc`, `refactor`, `workshop` add no extra new-test floor and require the\n  existing suite to remain green.\n\nThe active `Test Strategy` still applies in every scope and determines test\nvolume/types. Scope floors are additive; they never reduce or replace the\nselected strategy.\n\nBuild and Test verifies defined coverage floors and affirmed quality targets;\nthey may not be weakened to make a step pass.\n\nAffirm a stricter posture in `team.md` if the team commits to one."
    },
    {
      "layer": "project",
      "text": "- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14)"
    }
  ],
  "obligations": {
    "strategy": "standard",
    "strategy_volume": [
      "Five to eight tests per component.",
      "Unit tests plus integration tests for key boundaries.",
      "Add E2E, performance, or security tests when requirements demand them."
    ],
    "scope_floor": [
      "Keep the existing test suite green.",
      "This scope adds no extra new-test floor beyond the selected test strategy."
    ],
    "combination_rule": "Apply every selected-strategy obligation and every scope-floor obligation; neither replaces the other, and a targeted scope regression may add the narrowest necessary test type beyond the strategy default."
  },
  "plan_profile": {
    "methodology": "test-after",
    "runner_step": "Verify the existing test runner/configuration and record the exact unit-scoped command.",
    "runner_ready_before_first_test": true,
    "testable_layers": [
      "Data model / database behavior",
      "Repository / data access",
      "Business logic",
      "API / endpoint",
      "Frontend behavior"
    ],
    "steps": [
      "Project structure and production configuration skeleton.",
      "Verify the existing test runner/configuration and record the exact unit-scoped command.",
      "Data model / database behavior - implement.",
      "Data model / database behavior - write and run its tests after implementation.",
      "Repository / data access - implement.",
      "Repository / data access - write and run its tests after implementation.",
      "Business logic - implement.",
      "Business logic - write and run its tests after implementation.",
      "API / endpoint - implement.",
      "API / endpoint - write and run its tests after implementation.",
      "Frontend behavior - implement.",
      "Frontend behavior - write and run its tests after implementation.",
      "Environment/build configuration.",
      "Documentation and traceability."
    ]
  },
  "input_sha256": "sha256:269b5fe6c215f1c174eee59ab39199d705e1364320e780e6695b357b2faffa68",
  "contract_sha256": "sha256:3a38dc4eaaa9c318909bab63ee5df202dbcf2a67191fe3789eb17b5f2bd2ae2e"
}
```

Applicable testable layers for this unit: data model (registry schema),
repository/data access (`versioning.toml` + manifest readers), business logic
(validation rules BR1.1–BR1.8). No API/endpoint or frontend layer.

## Story-to-step traceability

- FR2.1/FR2.3 (registry schema, literal carriers) → Steps 3–5, 8
- FR2.2 (bidirectional completeness) → Steps 4, 6
- FR2.4 (non-component exclusion) → Steps 4, 5
- FR2.5/FR3.5 (baseline pending) → Steps 3, 8
- FR1.2/NFR4 (check output formats) → Steps 5, 7
- Migration (WF-3) → Steps 2, 9

## Plan steps

- [x] Step 1: Project structure — create `scripts/versioning/` layout
      (`cli.ts`, `lib/registry.ts`, `lib/workspace.ts`, `lib/glob.ts`,
      `commands/check.ts`) and confirm `bun test scripts/versioning` is the
      runnable unit-scoped test command (brownfield runner check).
- [x] Step 2: Write `versioning.toml` at repo root — `baseline_commit =
      "pending"`, `allowed_types`, `reserved_scopes`, and all 10 entries with
      correct `manifest_path`/`constant_path`, `tag_prefix`, `changelog_path`,
      `globs`, `scopes`, `depends_on` per the real workspace graph.
- [x] Step 3: Data model — `lib/registry.ts`: `RegistryConfig`/`RegistryEntry`
      types, strict-key TOML parse via `Bun.TOML.parse`, field validation,
      `loadRegistry()` returning `Result`-style outcome (error, never partial).
- [x] Step 4: Repository layer — `lib/workspace.ts`: enumerate Cargo members
      (`cargo metadata --no-deps`) and Bun workspace manifests (root
      `package.json` workspace globs → `apps/*`, `packages/*`); manifest version
      readers (Cargo `[package].version` literal, `package.json` `version`,
      `file#NAME` constant regex); non-component path rejection
      (templates/fixtures/schema markers).
- [x] Step 5: Business logic — validators for BR1.1 (kind→carrier), BR1.2
      (bidirectional completeness), BR1.3 (strict schema), BR1.4 (DAG
      acyclicity), BR1.5 (non-component exclusion), BR1.6 (baseline
      pending/sha40), BR1.7 (scope/tag_prefix/glob uniqueness incl.
      conservative segment-wise glob-overlap detection), each emitting
      `versioning.toml:<line>: <msg>` violations with line lookup.
- [x] Step 6: `commands/check.ts` — run all validators + workspace
      enumeration; dual-format output (text lines to stderr / `--json`
      violations); exit 0/1/2; `pending` baseline warns-but-passes.
- [x] Step 7: `cli.ts` entry — argv dispatch for `check`, usage errors → exit
      2; stable surface for later units (`plan`, `bump`, `changelog`,
      `commit-check`, `tag`).
- [x] Step 8: Tests — `lib/registry.test.ts` + `commands/check.test.ts` +
      `check.integration.test.ts` (runs `check` against the real repo).
      Cover: happy path, unknown key, kind/carrier xor, scope & tag-prefix
      collision, glob overlap, DAG cycle + unknown edge + self-edge, missing
      manifest, completeness both directions, baseline absent/malformed/
      pending, `--json` shape, exit codes. (~14 tests, Standard strategy.)
- [x] Step 9: One-time manifest migration — replace `version.workspace = true`
      with `version = "0.1.0"` in all 7 crate manifests; remove `version` from
      root `[workspace.package]`; add `pub const WIRE_VERSION: &str =
      "0.1.0";` in `crates/oxidean-core/src/lib.rs` beside
      `RPC_PROTOCOL_VERSION`; bun manifests already literal (verify only).
- [x] Step 10: Environment/build — `make version-check` target (+`.PHONY`)
      running `bun scripts/versioning/cli.ts check`; regenerate `Cargo.lock`
      after manifest rewrites (`cargo metadata`/`cargo check` refresh).
- [x] Step 11: Documentation/traceability — `code-summary.md`,
      `source-manifest.json`, `traceability.json`.
