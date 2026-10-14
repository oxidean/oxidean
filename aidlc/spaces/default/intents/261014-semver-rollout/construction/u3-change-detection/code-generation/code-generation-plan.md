# U3 Change Detection & Bump Engine — Code Generation Plan

Implements `detect`, `plan`, and `bump` — the release-plan pipeline (contract
C3). Consumes U1 (`loadRegistry`, workspace carriers) and U2 (`parseCommit`).
Methodology: test-after.

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

Applicable layers: business logic (attribution/propagation/bump math), CLI
surface (detect/plan/bump), repository layer (git enumeration). No data/API/
frontend layers.

## Decisions pinned

- **Per-component lower bound (FR3 + BR3.1):** global scan range is
  `--since <ref>` else `baseline_commit..HEAD` else repo-root while `pending`.
  Within that range a component only counts commits not reachable from its own
  latest `<tag_prefix>*` tag (when one exists) — `git rev-list <tag>..HEAD`
  intersection. `--since` overrides globally for all components.
- **Bump table (FR3.1):** `breaking` escalates to minor (<1.0.0) / major
  (>=1.0.0) regardless of type; otherwise `feat`→minor, `fix`/`perf`/`revert`→
  patch, `chore`/`docs`/`style`/`refactor`/`test`/`build`/`ci`→none.
- **Propagation (BR3.3/3.5, FR3.2):** bump origins = components whose direct
  bump is non-none. Dependent closure over `depends_on` reverse edges; reached
  entries record `propagatedFrom` = the origin ids. Final bump =
  `max(direct, patch)` when `propagatedFrom` non-empty, else `direct`.
- **Attribution (BR3.2):** `resolved scope ids ∪ glob-matched ids`; globs
  contribute even when scope is mapped (`source=scope_and_glob`); reserved
  scope → exempt; unmapped/absent scope → globs only; multi-entry glob match
  → all matches + `ambiguous` warning (defensive — BR1.7 normally prevents it).
- **toVersion (BR3.6, FR8.3):** `fromVersion` read from the live carrier
  (`readManifestVersion`/`readConstant`), never the registry. `stable` → bare
  semver; `alpha`/`beta` → `-<channel>.N`; `nightly` → `-nightly.YYYYMMDDN`;
  N = 1 + count of existing `<tag_prefix><base>-<channel>*` tags.
- **`bump` writes (BR3.7, FR3.3):** only `version = "…"` in Cargo.toml
  `[package]`, `"version": "…"` in package.json, `WIRE_VERSION` literal. Then
  unconditional lockfile regen: `cargo metadata --format-version=1` (refreshes
  Cargo.lock) + `bun install --lockfile-only`. Any write/regen failure → exit 1;
  applied edits are reported, never claimed clean on partial failure.
- **Plan output (BR3.8, C3):** `{schema:"release-plan", version:1, generatedAt,
  baseRef, entries[]}` — `baseRef` = HEAD sha; entries sorted by component id;
  only bump≠none entries listed (no forced-note mechanism exists — omitted).
  `plan` prints a human table; `plan --json` emits the C3 payload.
- **Exits:** 0 ok, 1 computation/write failure or violations, 2 usage/ref errors.

## Story-to-step traceability

- FR3 / FR3.5 / BR3.1-3.2 (range + attribution) → Steps 3, 4
- FR3.1 / BR3.4 (bump table) → Step 2
- FR3.2 / BR3.3/3.5 (propagation) → Step 4
- FR3.3 / BR3.7 (writer discipline + lockfiles) → Step 6
- FR8.3 / BR3.6 (channels) → Step 5
- BR3.8 / C3 (plan output) → Step 5

## Plan steps

- [x] Step 1: Verify test runner — `bun test scripts/versioning` covers this
      tree; record exact command in unit-test-instructions.md.
- [x] Step 2: `lib/semver.ts` — parse/compare/apply bump levels
      (`none<patch<minor<major`), 0.x-aware table, channel suffix math
      (pure; no I/O).
- [x] Step 3: `lib/git.ts` — injectable git layer: `rangeCommits` (`git log`
      sha+subject+body over a bound), `commitFiles` (`git diff-tree
      --no-commit-id --name-only -r`), `latestTag`/`tagCommits`
      (`git tag -l '<prefix>*'` + `rev-list -n1`/`rev-list <tag>..HEAD`),
      `headSha`. Ref failures surface as exit-2 errors.
- [x] Step 4: `lib/plan.ts` — `attribute(commits, registry)` → `Attribution[]`;
      `propagate(registry, directBumps)` → `propagatedFrom`; `computePlan`
      joining range + bounds + carriers → `BumpPlan` (deterministic modulo
      `generatedAt`, BR3.8).
- [x] Step 5: `commands/detect.ts` + `commands/plan.ts` — `detect [--since]
      [--json]` prints Attribution[]; `plan [--since] [--channel] [--json]`
      prints table or C3 JSON.
- [x] Step 6: `commands/bump.ts` — compute the same plan (never a divergent
      path), apply manifest/constant writes, regen lockfiles, print
      `ManifestEdit[]`; `--dry-run` prints the plan without writing.
- [x] Step 7: `cli.ts` — register `detect`, `plan`, `bump` + usage.
- [x] Step 8: Tests — `lib/semver.test.ts` (table incl. 0.x boundary + channel
      suffixes), `lib/plan.test.ts` (attribution union, exempt skip, multi-glob
      ambiguity, closure + propagatedFrom, max(direct,patch)), `commands/` tests
      over a temp `git init` fixture repo (range bound, tag bound, `plan
      --json` shape, `bump` writes + regen on a fixture manifest). ~20 tests.
- [x] Step 9: Docs/traceability — code-summary, source-manifest,
      traceability.json.
