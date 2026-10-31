# CI/CD Pipeline — u1-vcs-wire-foundation

No new lanes, no new jobs, no deployment surface. This unit's changes ride the existing CI pipeline; this document maps each change to the lane that gates it.

## Lane coverage

| Change | Lane | Gate |
|---|---|---|
| `vcs_types.rs` DTOs, `Nullable<T>`, `VcsErrorCode`, activity constants | `test` (cargo nextest) | serde round-trip tests (absent/null/value trichotomy, dotted codes, `ref` rename); compile of `oxidean-core` |
| `proc_classes` `vcs.*` block | `test` | classification coverage test (19-name mapping, fail-closed) |
| `PROCEDURES` + stub dispatch arms | `test` | `procedures_list_matches_dispatch`, `manifest_shape`, the extended `vcs.*` coverage test |
| `rpc_gen.rs` template + regenerated `packages/api-client` | `rpc-sync-check` | regen-diff parity; plus the new generated-coverage Rust test (17 names in `index.ts`) inside `test` |
| `RepoActivityItem` widening + `apps/web` null guards | `web-lint` (type-aware oxlint) + `web-format-check` + the web unit/integration Vitest lane (inside `web-octane`; the row's route coverage is happy-dom) | regenerated client's `string \| null` fields fail type-check without the guards |
| `crates/oxidean-vcs` skeleton + workspace member + `versioning.toml` `[[component]]` | `version-check` + `dead-code-check` | member↔component bidirectionality; literal `[package] version`; machete over zero-dep crate |
| `wire` SemVer bump for the breaking widening | `commit-lint` + `version-check` (attribution itself is review/release-plan enforced, not CI) | `wire` carries `globs = []` — the bump fires only on a `wire`- or `rpc`-scoped subject (a `BREAKING CHANGE:` footer on another scope never attributes to `wire`; the `!` marker only sets level). Pin `feat(wire)!: widen RepoActivityItem …` — `!` follows the scope paren. |
| `WIRE_VERSION` template↔const sync | `test` | new sync assertion (emitted literal == `oxidean_core::WIRE_VERSION`) — a Rust test, not a new lane. Third pin site noted: `docs/openapi.yaml` `info.version` — `bump.ts` doesn't rewrite it; release-time bumps run `make openapi-gen` (`openapi-sync-check` gates). |
| rustfmt/rustdoc on all new Rust | `rust-fmt` + `dead-code` job (`docs-check` shares it) | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` — new pub items must be rustdoc-clean |

## Sequencing constraint

`version-check` validates the landed tree, not per-commit history: workspace `members` entry, `[[component]]` registration, and the skeleton's literal-version `Cargo.toml` are CI-gated bidirectionally, while the `Cargo.lock` member delta is hygiene (no `--locked` gate exists) — all four belong in the **same landed tree**; one commit is the safe form (sequential pushes/rebase-merge/bisect would red-build intermediate trees).

*Lane naming: the table mixes Make targets (`test`, `rpc-sync-check`, …) and CI job names (`commit-lint`, `rust-fmt`, `dead-code`); `version-check`/`commit-lint` are steps of one CI job — used as logical lanes.*

## Rollback

Standard `git revert` — the unit adds no migrations, no runtime paths, no deployable surface; the stub-armed procedures return `vcs.disabled` and are inert until u7. One wrinkle: default revert subjects are attribution-exempt — for a wire-visible rollback use a hand-written `revert(wire):` (or `revert(wire)!:`) subject.
