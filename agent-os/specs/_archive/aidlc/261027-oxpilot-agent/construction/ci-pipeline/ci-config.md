# CI Configuration — oxpilot

The existing GitHub Actions pipeline is adequate and already covers this intent; it executes on every push to `feat/oxpilot-agent` (PR #203). No new pipeline is created — this document records the configuration the intent relies on.

## Jobs exercised by this intent (`ci.yml`)

| Job | What it runs for oxpilot |
|---|---|
| `commit-lint` | SemVer subjects across the PR range |
| `rust-build` | `cargo build/nextest/clippy/fmt` — all new crates (`oxidean-api-oxpilot`, `oxidean-cli` pilot, `oxidean-db` oxpilot, `oxidean-api-core`) |
| `rpc-sync` | generated `@oxidean/api-client` drift check — covers the 20 `oxpilot.*` procs |
| `web-octane` | `apps/web` build + `web-lint` + `web-format-check` incl. `.tsrx` parse via the intent's Vite virtual-module plugin fix |
| `web-browser` / `browser-coverage` | Chromium DOM-race tests; oxpilot manifest rows |
| `route-coverage` | `/oxpilot` + `/oxpilot/$session` manifest coverage |
| `dead-code` | knip + cargo machete + `clippy -D dead_code` |
| `db-dialects` | `dialect_oxpilot` probe per dialect (PG/MySQL/SQLite) — migrations `0087`–`0090` |
| `e2e-stack` | `oxpilot.stack.test.ts` — session create, POST /chat stream, flag-off denial |
| `coverage-weighted` | composite floor 0.70 |

`images.yml` builds Docker images; `ox-cli.yml` covers the CLI package (path-filtered).

## Environment

CI runs in `ghcr.io/oxidean/oxidean-ci:{dev,release,cov,tools}` images pinned by `ci-image` job (`scripts/ci-image-tag.sh`); no cross-run Rust cache by design. `OXIDEAN_OXPILOT_*` test values come from the e2e dev-auth env, never secrets.

## Sources

Consumes per-unit `code-summary` artifacts (`construction/*/code-generation/code-summary.md`), `build-and-test-summary` (`construction/build-and-test/build-and-test-summary.md`), and `build-test-results` (`construction/build-and-test/test-results.md`) — the target matrix there maps to the gates below.

## Execution policy for this intent

Local heavyweight builds are deferred per user directive; CI is the authoritative executor — the build-and-test Target Verification Matrix rows BT-1/2/3/6/7/8/15 are verified here. A green run on the PR head commit is the merge gate.
