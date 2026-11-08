# Units Generation — Questions

Decomposition planning for the semver rollout. Sources: `domain-design/components.md`, `domain-design/decisions.md`, `requirements.md`, `scope-definition/intent-backlog.md`.

## Q1. Unit boundary strategy — how are the 11 components grouped into construction units?

Options:
- A) **By cohesion seam** — group components that share a codebase surface and a test story: registry schema/parser (VersionRegistry), grammar+parser (CommitGrammar), the detection→plan→write engine (ChangeDetector+DependencyGraph+BumpPlanner+ManifestWriter), changelog+release (ChangelogWriter+ReleaseOrchestrator), enforcement surfaces (CommitLintGate), in-repo wiring (VersionSurfaceWiring), docs (ContributorGuidance) → 7 units.
- B) One unit per component → 11 units, more gates and contract ceremony per unit.
- C) One unit per proto-backlog row → 14 units, splits shared code paths across units.

[Answer]: A — cohesion seams. The 14 proto-units collapse onto 6 component clusters that share files/tests (e.g. CommitGrammar's parser is inside the same Bun CLI as the detector and bump planner — separate units would create artificial contract surfaces inside one module). 7 units keeps each unit bolt-sized without splitting the `scripts/versioning/` tool across unit boundaries. Conductor-decided under standing continuation instruction; ratify at PR review.

## Q2. Unit granularity — coarse or fine?

Options:
- A) Bolt-sized: each unit is one focused implementation slice a single pass can complete and test.
- B) Coarser: merge engine+changelog+release into one big unit.

[Answer]: A — bolt-sized. The largest cluster (detection→bump engine) is already at the top of the comfortable size; folding changelog+release in would make it untestable in one pass.

## Q3. Dependency ordering — strict topological only, or parallel where independent?

Options:
- A) Topological DAG only — `units-generation` describes what CAN depend on what; sequencing is delivery-planning's job (skipped → linear DAG order applies).
- B) Annotate a preferred sequence here anyway.

[Answer]: A — topology only, per stage contract. Delivery-planning is skipped in this scope, so the DAG order itself is the build order; no critical-path claims made here.

## Q4. Integration points between units — what do the units exchange?

The shared contract is: `versioning.toml` schema (registry) → the Bun CLI's subcommand surface (`commit-check`, `plan`, `bump`, `changelog`, `tag`, `check`) → workflow/Make targets calling them → in-repo constant/surface rewiring. Units integrate through the CLI's subcommands and the registry file format, not through runtime APIs.

Options:
- A) Confirm: units integrate through the Bun CLI subcommand boundary + the TOML registry schema, defined in contract-design.
- B) Something else.

[Answer]: A — confirmed. The inter-unit contract is exactly the CLI subcommand surface + registry schema + tag format, which contract-design (2.8) formalises.

## Q5. Deployment model per unit?

All units produce repo artifacts (scripts, hooks, workflows, source edits, docs) — nothing deploys independently; the units land together on one branch and merge as one PR.

Options:
- A) Confirm: shared deployment — single PR, `embedded` model for all units.
- B) Per-unit deployment.

[Answer]: A — shared/embedded. One PR carries all units; "deployment" is the tag-driven release flow the work itself creates.

## Q6. Nice-to-have backlog placement — which units absorb PR-title lint, release-PR bot, prerelease, annotated tags?

Options:
- A) Fold into the units that own the machinery: PR-title lint → commit-lint-gates unit; release-PR workflow → changelog-release unit; prerelease levels → detection/bump engine (ReleasePlanEntry.prerelease); annotated tags → changelog-release (ADR-002 already picks annotated).
- B) Leave them out entirely.

[Answer]: A — folded, matching the intent backlog (U11→lint gate, U12→release unit, U13→engine, U14→release unit). They're implemented where their machinery lives rather than as standalone units.

---

## Consolidated Summary Confirmation

- Boundary strategy: cohesion seams → **7 units** (U1 version-registry, U2 commit-grammar, U3 change-detection-bump, U4 changelog-release, U5 commit-lint-gates, U6 version-surfaces, U7 contributor-guidance).
- Granularity: bolt-sized.
- Ordering: topological DAG only; no economic sequencing here.
- Integration: Bun CLI subcommand surface + `versioning.toml` schema + tag format (formalised in contract-design).
- Deployment: single shared PR, all units embedded.
- Nice-to-haves folded into their machinery-owning units.

Does this all look correct before I generate the unit-of-work artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
