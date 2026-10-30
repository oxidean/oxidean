# Quality Gates — semver-release-automation

## Merge gates (ci.yml, required before merge)

| Gate | Enforces | Maps to Build-and-Test evidence |
|---|---|---|
| `commit-lint` | `versioning commit-check` on PR commit range + PR title; `versioning check` registry completeness | T1, T3 |
| `api-rust` | `cargo nextest run --workspace --profile ci` | T4–T6 locally; full matrix here |
| `web-octane` | web lint + format + Vitest + Turbo build | T7, T10 |
| `route-coverage` | `make route-coverage-check` | unchanged surface |
| `web-browser` | Chromium component suite | unchanged surface |
| `browser-coverage` | `make browser-coverage-check` | unchanged surface |
| `coverage-weighted` | `make coverage-contract/rust/web` | unchanged surface |
| `e2e-stack` | `make test-e2e-stack` | unchanged surface |
| `rpc-sync` | `make rpc-sync-check` (regen + committed-tree diff) | T8 (gate becomes authoritative post-commit) |
| `compose` / `compose-smoke` / `smoke-protocol` / `db-matrix` / `docker-ci-images` | stack syntax + bring-up + protocol + dialect | T12 locally for syntax; runtime here |
| `skill-sync-check` | `make skill-sync-check` (ox skill ↔ CLI surface) | unchanged surface |

## Release gates

- `release.yml` refuses tag pushes without `secrets.RELEASE_TOKEN`.
- `ox-cli.yml` writes `latest.json` only on stable (non-prerelease) cuts.
- `release-pr.yml` no-ops cleanly when the bump plan is empty
  (`bumped == 0` guard before branch/PR).
- All workflows parse under `act -l`; embedded bash validated with
  `bash -n` (111 blocks, T11/T13).

## Bypass posture (NFR5)

- `--no-verify` skips the local hook only — CI `commit-lint` re-checks
  the full PR range, so local bypass cannot weaken enforcement.
- `make setup` is the only mutation of git config and announces itself.
- Required-check protection on `main` (branch rules) is owned by repo
  settings, outside this scope's writable surface — noted for reviewers.

## Gate coverage of Build-and-Test commands

Every command the Build and Test stage recorded has a CI owner:
`version-check`/`commit-check` → `commit-lint`; Rust tests → `api-rust`;
Vitest/web lint → `web-octane`; rpc-sync → `rpc-sync`; compose syntax →
`compose`; workflow shape → validated statically (no CI equivalent —
workflow edits go through the same PR gates).

## Sources

Consumes the per-unit `code-summary.md` files under
`construction/*/code-generation/` and this stage's upstream
`build-and-test/build-and-test-summary.md` +
`build-and-test/test-results.md` (the build-test-results surface).

