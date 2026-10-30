# Cross-Unit Traceability — semver-release-automation

**Verdict: PASS** — all 50 enumerated FR/NFR ids from
`inception/requirements-analysis/requirements.md` are covered `OK` in at
least one unit's `traceability.json`, and every cited target file exists
(checked on disk). Three-segment `AC` ids: user-stories stage did not run
for this scope — none enumerated.

Coverage was completed during this stage: umbrella ids (FR1/FR2/FR5/FR8)
and the NFRs no unit had claimed (NFR1/NFR2/NFR5/NFR6, plus FR1.3) were
back-filled into the owning units' traceability files with real targets —
they were planning-record gaps, not implementation gaps.

| ID | Status | Owning unit | Target file | Exists |
|---|---|---|---|---|
| FR1 | OK | u1-version-registry | `versioning.toml` | yes |
| FR1.1 | OK | u2-commit-grammar | `scripts/versioning/lib/commit.ts` | yes |
| FR1.2 | OK | u5-commit-lint-gates | `.github/workflows/ci.yml` | yes |
| FR1.3 | OK | u2-commit-grammar | `scripts/versioning/lib/commit.ts` | yes |
| FR2 | OK | u1-version-registry | `versioning.toml` | yes |
| FR2.1 | OK | u1-version-registry | `crates/oxidean-core/Cargo.toml` | yes |
| FR2.2 | OK | u1-version-registry | `versioning.toml` | yes |
| FR2.3 | OK | u1-version-registry | `crates/oxidean-core/src/lib.rs` | yes |
| FR2.4 | OK | u1-version-registry | `scripts/versioning/lib/workspace.ts` | yes |
| FR2.5 | OK | u1-version-registry | `versioning.toml` | yes |
| FR3 | OK | u3-change-detection | `scripts/versioning/lib/plan.ts` | yes |
| FR3.1 | OK | u2-commit-grammar | `scripts/versioning/lib/commit.ts` | yes |
| FR3.2 | OK | u3-change-detection | `scripts/versioning/lib/plan.ts` | yes |
| FR3.3 | OK | u3-change-detection | `scripts/versioning/commands/bump.ts` | yes |
| FR3.4 | OK | u4-changelog-release | `.github/workflows/release.yml` | yes |
| FR3.5 | OK | u3-change-detection | `scripts/versioning/lib/plan.ts` | yes |
| FR4 | OK | u4-changelog-release | `scripts/versioning/commands/changelog.ts` | yes |
| FR4.1 | OK | u4-changelog-release | `CHANGELOG.md` | yes |
| FR4.2 | OK | u4-changelog-release | `scripts/versioning/lib/changelog.ts` | yes |
| FR4.3 | OK | u4-changelog-release | `docs/WIRE-CHANGELOG.md` | yes |
| FR4.4 | OK | u4-changelog-release | `scripts/versioning/lib/changelog.ts` | yes |
| FR5 | OK | u4-changelog-release | `.github/workflows/release.yml` | yes |
| FR5.1 | OK | u4-changelog-release | `.github/workflows/ox-cli.yml` | yes |
| FR5.2 | OK | u4-changelog-release | `.github/workflows/publish-runner.yml` | yes |
| FR5.3 | OK | u4-changelog-release | `.github/workflows/release.yml` | yes |
| FR5.4 | OK | u4-changelog-release | `.github/workflows/release.yml` | yes |
| FR5.5 | OK | u4-changelog-release | `.github/workflows/release.yml` | yes |
| FR5.6 | OK | u4-changelog-release | `.github/workflows/ox-cli.yml` | yes |
| FR6 | OK | u6-version-surfaces | `crates/oxidean-core/src/lib.rs` | yes |
| FR6.1 | OK | u6-version-surfaces | `crates/oxidean-cli/src/rpc.rs` | yes |
| FR6.2 | OK | u6-version-surfaces | `crates/oxidean-api/src/rpc.rs` | yes |
| FR6.3 | OK | u6-version-surfaces | `crates/oxidean-api/src/routes/cli_dist.rs` | yes |
| FR6.4 | OK | u6-version-surfaces | `crates/oxidean-core/src/lib.rs` | yes |
| FR6.5 | OK | u6-version-surfaces | `.env.example` | yes |
| FR6.6 | OK | u6-version-surfaces | `crates/oxidean-api/src/bin/rpc_gen.rs` | yes |
| FR7 | OK | u7-contributor-guidance | `docs/VERSIONING.md` | yes |
| FR7.1 | OK | u7-contributor-guidance | `docs/VERSIONING.md` | yes |
| FR7.2 | OK | u7-contributor-guidance | `.agents/skills/semver-conventions/SKILL.md` | yes |
| FR7.3 | OK | u7-contributor-guidance | `Makefile` | yes |
| FR8 | OK | u4-changelog-release | `.github/workflows/release-pr.yml` | yes |
| FR8.1 | OK | u5-commit-lint-gates | `.github/workflows/ci.yml` | yes |
| FR8.2 | OK | u4-changelog-release | `.github/workflows/release-pr.yml` | yes |
| FR8.3 | OK | u3-change-detection | `scripts/versioning/lib/semver.ts` | yes |
| FR8.4 | OK | u4-changelog-release | `scripts/versioning/commands/release-tags.ts` | yes |
| NFR1 | OK | u1-version-registry | `scripts/versioning/cli.ts` | yes |
| NFR2 | OK | u4-changelog-release | `scripts/versioning/lib/changelog.ts` | yes |
| NFR3 | OK | u1-version-registry | `versioning.toml` | yes |
| NFR4 | OK | u1-version-registry | `scripts/versioning/lib/registry.test.ts` | yes |
| NFR5 | OK | u5-commit-lint-gates | `.githooks/commit-msg` | yes |
| NFR6 | OK | u4-changelog-release | `.github/workflows/ox-cli.yml` | yes |

## Uncovered elements

None.

## Sources

Consumes per unit `code-generation-plan.md`, `unit-test-instructions.md`,
and `code-summary.md` under `construction/*/code-generation/` (all seven
units), plus the stage-level Testing Contract they embed.

