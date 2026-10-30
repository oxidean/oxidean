# Quality Gates — oxpilot

## Merge-blocking gates (required status checks)

Every `ci.yml` job plus `images.yml` `meta` must pass on the PR head commit before merge (Main branch ruleset). Mapping to build-and-test targets:

| Gate | Covers |
|---|---|
| `rust-build` | BT-1/2/3 — nextest suites incl. transcript/summary bounds, timeouts, round caps; clippy/fmt/doc |
| `db-dialects` | BT-1 — `dialect_oxpilot` across PG/MySQL/SQLite |
| `e2e-stack` | BT-15 — stack-level stream contract, flag-off denial |
| `rpc-sync` | BT-8 — generated client in sync with Rust procs |
| `web-octane`, `web-browser`, `browser-coverage`, `route-coverage` | BT-9/10 CI-side + BT-11/12 (also verified locally) |
| `dead-code` | knip/machete/dead_code across new exports |
| `coverage-weighted` | coverage floor 0.70 |
| `commit-lint` | SemVer commit subjects |
| security checks (BT-6/7) | distributed: `oxidean-api --test oxpilot` isolation/PAT-denial/flag-off tests run under `rust-build`; `oxidean-core redact` tests same job |

## Non-blocking / later-owned

- `performance-validation` (operation phase): BT-4/BT-5 p99 latency SLOs need a deployed instrumented stack — explicitly out of PR CI scope.
- CodeRabbit review on PR #203: human-requested advisory gate; findings fixed until no actionable items remain.
