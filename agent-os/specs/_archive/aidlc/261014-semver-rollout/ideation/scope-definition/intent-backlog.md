# Intent Backlog — Semver & Release Automation

Upstream: `ideation/intent-capture/intent-statement.md`, `ideation/scope-definition/scope-document.md`
Priorities: MoSCoW. Ordering: dependency-first (confirmed in scope-definition Q6).

## Proto-Units

| ID | Capability | Priority | Depends on |
|----|-----------|----------|------------|
| U1 | Component inventory + `0.1.0` baselines — enumerate every Cargo crate and Bun app/package, set independent versions | Must | — |
| U2 | SemVer commit-prefix convention spec — grammar + bump mapping (incl. `0.x` semantics) | Must | — |
| U3 | Dependency-aware changed-component detection — graph across Cargo + Bun, shared-dep changes propagate to dependents | Must | U1, U2 |
| U4 | Version-bump automation — per-component bump computation from prefixed commits + detection | Must | U2, U3 |
| U5 | Changelog generation — per-component changelogs + one global consolidated changelog referencing them | Must | U4 |
| U6 | Release wiring — tag strategy + integration into the existing `ox-cli.yml` tag-triggered flow | Must | U4, U5 |
| U7 | Git commit-msg hook + distribution mechanism | Must | U2 |
| U8 | CI enforcement — server-side commit-message lint so `--no-verify` can't bypass | Must | U2 |
| U9 | Version surfaces — API/RPC protocol version + consistent reporting in CLI self-update, MCP, API, manifests | Must | U1 |
| U10 | Agent skills/helpers + contributor docs for the practice | Must | U2 |
| U11 | PR-title linting for the same convention | Should | U2, U8 |
| U12 | Automated release PRs / bot-opened bump branches | Could | U4, U6 |
| U13 | Prerelease channels (alpha/beta/nightly schemes) | Could | U4 |
| U14 | Signed or annotated release tags | Could | U6 |

## Notes

- All Must items are required for the initiative to count as done (Q4).
- U3 is the highest-uncertainty item — change detection spanning two toolchains — but the confirmed order is dependency-first, not risk-first.
- Verification posture per constraint register: docker + act for CI/Docker changes; hook accept/reject, bump logic (incl. `0.x`), changelog generation, and version-surface consistency all get tests.
