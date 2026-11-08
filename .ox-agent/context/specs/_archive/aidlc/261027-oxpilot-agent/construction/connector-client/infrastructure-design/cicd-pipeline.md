# CI/CD Pipeline — connector-client (U7)

Consumes: `infrastructure-design-questions.md` (Q2-A/Q3-A/Q5-A),
`team.md` Testing Posture + Deployment + Code Style,
`.github/workflows/{ci,ox-cli}.yml`, `scripts/check-cli-parity.sh`,
`docs/CLI-PARITY.md`, `crates/oxidean-cli/Cargo.toml`.

**No new jobs, no ruleset edit.** One workflow-file change: a
single step is added inside the existing `rpc-sync` job (below) so
the parity gate actually fires — everything else is landing the
code where the filters already see it.

## Lane coverage

| lane | what covers U7 | evidence it fires |
|------|----------------|-------------------|
| `ci.yml` `rust-build` → `api-rust` shards | `cargo nextest archive --workspace --features server` builds one archive; `tests/pilot_connector.rs` (wiremock C3 suite) + `src/pilot/*` unit tests (jail chains, env filter, tail buffer, scrub set, settled dedupe, custody backends on temp dirs) replay in the shards. **No `-E` filterset term needed** — the archive is workspace-wide (contrast: `db-dialects`'s binary_id-inclusion filterset is why U6's tests had to live inside `dialect_oxpilot.rs`; no such gate applies to the crate's `tests/` + colocated unit tests). | `ci.yml` nextest-archive step; `.config/nextest.toml` ci profile (retries=1, 90s slow-timeout) |
| `ox-cli.yml` `build` matrix | `paths:` includes `crates/oxidean-cli/**` — any PR touching `src/pilot/` fires all 9 matrix legs + the FreeBSD VM leg. `cmd /C` + `taskkill /T` compile+smoke on `windows-latest`/`windows-11-arm`; `sh -c` + `killpg`/`process_group` on the unix legs; `cross` legs compile the musl/armv7 variants | `ox-cli.yml` `on.pull_request.paths`, matrix `include`, smoke steps (`ox-smoke.sh`/`ox-smoke.ps1`, qemu-container smokes, Rosetta leg) |
| `scripts/check-cli-parity.sh` — **wired in this change** | `docs/CLI-PARITY.md` rows marked `implemented` must parse `ox <group> --help`. **The check currently has no CI caller** — `Makefile` exposes `cli-parity-check` but no workflow invokes it. The same change adds one step inside the existing `rpc-sync` job (which already downloads `rust-bins` incl. `target/debug/ox` and sets `OXIDEAN_PREBUILT_BINS=1`): `OX_BIN=target/debug/ox ./scripts/check-cli-parity.sh`. The `pilot` group needs rows for `pair`/`run`/`disconnect`/`status` in the same change — the probe is **group-level** (`ox pilot --help`; the awk collapses all four rows to the group via `sort -u`), so an `implemented` row whose *group* doesn't parse fails; per-verb registration is docs completeness, not gate-verified. | `check-cli-parity.sh` awk extract (`$4 ~ /implemented/`, first backticked token) + `ox $group --help` probe grepping "unknown command"; `rpc-sync` job gains the step |
| `rust-fmt` / `dead-code` / `clippy` jobs | workspace-wide — `src/pilot/*` inherits `cargo fmt`, `clippy --workspace --all-targets -D warnings`, `cargo machete`, `RUSTDOCFLAGS="-D warnings"` | `ci.yml` formatting/dead-code jobs |
| `commit-lint` | PR title + range must match the SemVer grammar; the pilot work lands under `feat(cli)` / `feat(oxidean-cli)` scope | `ci.yml` commit-lint job |

## Docs surfaces (same-change registrations)

| surface | edit | why |
|---------|------|-----|
| `docs/CLI-PARITY.md` | `pilot` group rows: `pair`, `run`, `disconnect`, `status` marked `implemented` (ox side; gh column `—`) — `implemented`, not `ox-only`: the awk probes only `implemented` rows, so `ox-only` would exempt `pilot` from the wired gate | the parity check (newly wired into `rpc-sync`) probes every implemented row's group |
| `docs/CLI.md` | `## Commands` section gains the `pilot` verbs — the section enumerates every `ox` group exhaustively; a shipped `pilot` group absent there is a user-visible reference gap | the doc's own "readable without source" contract |
| `crates/oxidean-cli/skill/SKILL.md` | **deliberately excluded** — the cheat sheet covers agent-driven commands; the connector lifecycle is operator-run. Noted because touching it would trip `skill-sync-check` (rpc-sync job — `check-skill-sync.sh` validates every `ox …` mention + flag against `ox completion commands`/`--help`) | recorded so the decision is conscious, not an omission |
| `docs/CONNECTOR.md` (or `docs/CONFIGURATION.md` connector section) | the operator runbook: pair→run flow, jail semantics (fs_* jailed vs exec cwd-jailed + ambient-shell caveat), cap table (64 KiB tail / 128 KiB FS_IO_CAP / 256 KiB OUTPUT_MAX / 300s EXEC_TIMEOUT), exit-code contract, custody model (keyring→file fallback), disconnect-is-local-forget | the contract must be readable without source |
| `--help` text | exit-code contract + jail semantics + caps, verbatim per observability-design | supervisor authors read `--help`, not docs |
| `src/pilot/openapi.yaml` | checked-in reference of the consumed C3 surface — **reference doc, not a generated/drift-gated artifact** (no `openapi-sync-check` row; it documents what the server already ships) | keeps the client-side contract readable |
| `.env.example` / env docs | **untouched** — zero new env vars client-side | custody is keyring/files, not env |

## Supply chain

- New deps (`secrecy`/`zeroize`, `base64`, killpg path per the
  nfr-design deps table) are ordinary cargo deps: pinned versions,
  committed `Cargo.lock`, `cargo install --locked` in CI,
  dependabot's weekly cargo batch for freshness. `base64` matches
  the workspace's existing pin rather than introducing a second.
- `wiremock = "0.6.5"` already sits in `oxidean-cli`'s
  `[dev-dependencies]` — the C3 suite needs no new test infra.

## Explicitly NOT in this unit

- No stack-e2e lane: the connector needs a live forge to pair; the
  wiremock suite is the contract coverage (NFR9's client half).
  Full pair→run stack coverage belongs to the e2e story, not a
  U7-scoped job.
- No ruleset/required-checks edit: nothing here adds, renames, or
  removes a CI job — the merge gate list is untouched.
- No new `ox` packaging: the verbs ride the existing binary,
  matrix, checksums, and `ox-v*` release lane.
- No coverage-floor change: the weighted gate consumes web numbers
  only; Rust-side coverage stays collect-only per the affirmed
  posture.
