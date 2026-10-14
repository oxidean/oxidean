# Unit of Work — Story Map

User Stories (2.4) was skipped in this scope, so functional requirements (FRs) substitute for stories: each FR maps to its implementing Unit. Coverage rule: every FR assigned; every unit carries at least one FR.

| FR | Title | Unit | Directory |
|---|---|---|---|
| FR1 | Commit grammar contract | U2 | u2-commit-grammar |
| FR1.1 | Local commit-msg hook | U5 | u5-commit-lint-gates |
| FR1.2 | CI commit + PR-title lint | U5 | u5-commit-lint-gates |
| FR1.3 | Exempt subjects | U2 | u2-commit-grammar |
| FR2 | Nine manifests + wire entry versioned | U1 | u1-version-registry |
| FR2.1 | Literal manifest versions | U1 | u1-version-registry |
| FR2.2 | `versioning.toml` registry + completeness check | U1 | u1-version-registry |
| FR2.3 | Wire entry scope-driven detection | U1 | u1-version-registry |
| FR2.4 | Non-component versions never rewritten | U1 | u1-version-registry |
| FR2.5 | Baseline tag set = 0.1.0 cut | U4 | u4-changelog-release |
| FR3 | Detection → attribution → propagation pipeline | U3 | u3-change-detection |
| FR3.1 | 0.x-aware bump table | U3 | u3-change-detection |
| FR3.2 | Patch-level propagation rule | U3 | u3-change-detection |
| FR3.3 | Manifest rewrites + lockfile regen | U3 | u3-change-detection |
| FR3.4 | Component-prefixed annotated tags, no GITHUB_TOKEN cascades | U4 | u4-changelog-release |
| FR3.5 | Baseline-bounded history scan | U3 | u3-change-detection |
| FR4 | Per-component Keep a Changelog | U4 | u4-changelog-release |
| FR4.1 | Consolidated root CHANGELOG.md | U4 | u4-changelog-release |
| FR4.2 | Idempotent changelog generation | U4 | u4-changelog-release |
| FR4.3 | Wire changelog at docs/WIRE-CHANGELOG.md | U4 | u4-changelog-release |
| FR4.4 | `### Dependencies` on propagated bumps | U4 | u4-changelog-release |
| FR5 | Tag-driven release integration | U4 | u4-changelog-release |
| FR5.1 | ox-cli.yml retargeting + path-filter fix + changelog notes | U4 | u4-changelog-release |
| FR5.2 | publish-runner.yml component tag | U4 | u4-changelog-release |
| FR5.3 | `make release*` entrypoints | U4 | u4-changelog-release |
| FR5.4 | GitHub Release per component tag | U4 | u4-changelog-release |
| FR5.5 | Generic `*-v*.*.*` release job (runner + wire lanes) | U4 | u4-changelog-release |
| FR5.6 | Prefix-stripped version extraction everywhere | U4 | u4-changelog-release |
| FR6 | Component-aware version reporting | U6 | u6-version-surfaces |
| FR6.1 | CLI surfaces (`--version`, UA, clientInfo, self-update) | U6 | u6-version-surfaces |
| FR6.2 | API surfaces (health, manifest, serverInfo) | U6 | u6-version-surfaces |
| FR6.3 | `/cli/latest` + dist manifest → CLI version | U6 | u6-version-surfaces |
| FR6.4 | Wire SemVer unification + compat predicate + MCP pins | U6 | u6-version-surfaces |
| FR6.5 | Dead `OXIDEAN_RPC_VERSION` removal | U6 | u6-version-surfaces |
| FR6.6 | api-client + web independent versions | U6 | u6-version-surfaces |
| FR7 | Convention documentation | U7 | u7-contributor-guidance |
| FR7.1 | CONTRIBUTING + docs updates | U7 | u7-contributor-guidance |
| FR7.2 | `.agents/skills` semver skill + AGENTS.md | U7 | u7-contributor-guidance |
| FR7.3 | Makefile help coverage | U7 | u7-contributor-guidance |
| FR8 | Nice-to-haves riding the same machinery | U4 | u4-changelog-release |
| FR8.1 | PR-title linting | U5 | u5-commit-lint-gates |
| FR8.2 | Automated release-PR workflow | U4 | u4-changelog-release |
| FR8.3 | Prerelease suffixes | U3 | u3-change-detection |
| FR8.4 | Annotated (+ optionally signed) tags | U4 | u4-changelog-release |

## Cross-cutting notes

- The `scripts/versioning/` Bun CLI physically hosts U1's loader, U2's parser, U3's planner, U4's emitters, and U5's `commit-check` — the units own *subcommand surfaces* of one tool (ADR-005), not separate packages. Contract-design formalises the shared interfaces so the surfaces don't drift.
- FR2.3's wire-entry registry row lands in U1; its *detection behaviour* executes in U3's engine — the row is data U3 reads.
- FR2.4 ("non-component versions never rewritten") is split the same way: U1's registry glob scoping is the declared boundary, but U3's ManifestWriter carries the writer discipline + tests ("rewrites only in-registry version fields — never fixtures, templates, or schema markers").
- FR6.4's MCP-pin dedup touches five sites across four components — all inside U6's single wiring pass.

## Coverage verification

- Every FR (FR1–FR8.4, 44 ids) appears exactly once above; no FR is unassigned.
- Every unit carries at least one FR: U1 (FR2.x), U2 (FR1/1.3), U3 (FR3.x, FR8.3), U4 (FR2.5, FR3.4, FR4.x, FR5.x, FR8.2, FR8.4), U5 (FR1.1, FR1.2, FR8.1), U6 (FR6.x), U7 (FR7.x).
