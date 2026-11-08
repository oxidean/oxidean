# Requirements Analysis — Questions

Mode note: this stage ran while the user was away under a standing instruction — "keep going up to the point of having tested all changes locally and having an open PR with green CI ready for review". The questions below are the technical-implementation decisions ideation deliberately deferred (tool choice, commit grammar, hook mechanism, tag scheme, CI enforcement depth). Each answer records the conductor's decision, its rationale, and the code evidence it rests on; the user ratifies or overturns any of them at PR review.

## Q1. What commit-message grammar drives change detection?

Context: the version bumper, changelog generator, commit-msg hook, and CI lint must all agree on one grammar. The repo has no existing convention (no hook, no commit lint); `git log` shows ad-hoc free-form subjects.

- A. Conventional Commits — `type(scope)!: description` with types feat/fix/chore/docs/style/refactor/perf/test/build/ci/revert, optional scope, `!` or `BREAKING CHANGE` footer for breaking
- B. A custom Oxidean prefix vocabulary (e.g. `feat/`, `fix/` in the subject)
- C. GitHub-style tag prefixes (`[feat]`, `[fix]`)
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — Conventional Commits. It is the standard grammar every changelog tool and commit linter already parses, the scope field doubles as the component-attribution channel, and it is what "semver prefixed commits" idiomatically means. Conductor-decided under standing continuation instruction (2026-10-14); ratify at PR review.

## Q2. How do prefixes map to SemVer bumps while components sit at 0.x?

Context: SemVer's 0.x rules differ — nothing below 1.0.0 promises stability, so breaking→major is wrong today but must flip correctly after a component reaches 1.0.0.

- A. 0.x-aware mapping — while a component is <1.0.0: breaking (`!`/BREAKING CHANGE) and feat → minor; fix/perf/revert → patch; chore/docs/style/refactor/test/build/ci → no bump. At ≥1.0.0 the standard mapping applies (breaking → major)
- B. Breaking always → major even at 0.x (e.g. 0.1.0 breaking → 1.0.0)
- C. Everything → patch at 0.x
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — 0.x-aware mapping. Breaking changes must never silently promote a component to 1.0.0 (a maturity claim the project hasn't made); at 0.x they map to minor per the semver convention for unstable majors, and the ≥1.0.0 branch future-proofs the same rule table. Conductor-decided (2026-10-14); ratify at PR review.

## Q3. How does a commit declare which component(s) it affects?

Context: ten components share one history; a commit must resolve to an affected-component set before dependency propagation runs.

- A. Hybrid — the conventional `scope` field maps to components via an explicit registry (scope aliases → component ids); when scope is absent, changed-file paths resolve components via glob rules; the internal dependency graph then propagates hits to dependents
- B. Scope field only — commits without a valid scope are rejected by the hook
- C. Changed-file paths only — no scope mapping at all
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — hybrid. Requiring a scope on every commit is fragile (merge commits, reverts, bot commits) and path-only detection can't express "this core change matters to api"; scope-declared intent wins when present, path globs cover the rest, and the dependency DAG always propagates — matching the user's "boundary checks should be smart as to also detect shared crates hits on their dependencies". Conductor-decided (2026-10-14); ratify at PR review.

## Q4. Where does each component's authoritative version live?

Context: today all Cargo crates inherit `workspace.package.version`, so the Cargo.toml files cannot carry independent numbers as-is.

- A. Per-manifest authority — each component's own manifest (`Cargo.toml` `version`, `package.json` `version`) holds its literal version and is the single source of truth; `version.workspace = true` is replaced by literals; a root versioning config (`versioning.toml`) declares the component registry — ids, manifest paths, change-detection globs, scope aliases, tag names — but does NOT duplicate the numbers
- B. Central `versions.toml` — one file owns every number; manifests are regenerated/synced from it
- C. Keep workspace inheritance for crates and version only the leaf deliverables
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — per-manifest authority. Native tooling (`cargo`, `npm`, `cargo install --locked`, docs generators) already reads the real manifests, so the number that ships is the number of record — no sync step that can drift. The registry file only holds metadata (paths, globs, tag names), never versions. Conductor-decided (2026-10-14); ratify at PR review.

## Q5. What do release tags look like for ten independently-versioned components?

Context: existing automation triggers on `v*.*.*` (ox CLI distribution, runner image publish) — per-component tags must coexist with that pattern and disambiguate which component a tag releases.

- A. `<component>-v<semver>` — e.g. `oxidean-api-v0.2.0`, `ox-v0.3.1`, `web-v0.1.1`; annotated tags; the `v*.*.*` trigger is extended to the per-component pattern set
- B. `<component>@<semver>` — npm-style, e.g. `oxidean-api@0.2.0`
- C. One flat `v<semver>` tag that releases every bumped component together (single global version train)
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — `<component>-v<semver>` annotated tags. `@` tags work but render poorly in GitHub release URLs and clash visually with npm version syntax; a flat tag would collapse independent versioning into a single train, contradicting the approved per-component decision. `ox-dist` rolling tag stays as-is. Conductor-decided (2026-10-14); ratify at PR review.

## Q6. What builds the bump + changelog automation?

Context: the required behavior — conventional-commit parsing, scope/path attribution, cross-toolchain dependency propagation (Cargo + Bun), per-component and consolidated changelogs — has no off-the-shelf fit (changesets is JS-only; release-plz is Cargo-centric; git-cliff has no dependency-propagation model).

- A. Repo-owned tool — a Bun/TypeScript script under `scripts/` (Bun is already the repo runtime; runs in CI with zero new dependencies); it owns commit parsing, the component registry, the dependency DAG, bump computation, manifest rewriting, and changelog emission
- B. git-cliff configured per-component path globs + custom propagation wrapper
- C. Two tools — release-plz for crates, changesets for Bun packages
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — repo-owned Bun tool. Two half-tools would double the config surface and still not deliver dependency-aware propagation or a consolidated changelog; a ~script owns the whole policy in one place, testable with Vitest under the existing `make test` harness. Conductor-decided (2026-10-14); ratify at PR review.

## Q7. How is the commit-msg git hook distributed?

Context: `.git/hooks` is untracked, so enforcement needs a distribution mechanism contributors actually run — plus CI as the non-bypassable backstop.

- A. Tracked `.githooks/commit-msg` + `git config core.hooksPath .githooks` set by a documented `make setup` target (idempotent); CI commit-lint is the real enforcement — the hook is local ergonomics
- B. Husky/lint-staged via package.json (adds JS hook tooling to the Rust/Bun repo)
- C. Document the manual `cp` of a script into `.git/hooks`
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — `.githooks/` + `make setup` wiring `core.hooksPath`. Zero new dependencies, one command for contributors, and honesty about the security model: client hooks are bypassable (`--no-verify`), so the Must-have enforcement lives in CI and the hook only gives fast local feedback. Conductor-decided (2026-10-14); ratify at PR review.

## Q8. How deep does CI enforcement go?

Context: local hooks can be skipped; the server side decides whether malformed subjects can merge.

- A. Lint every commit in the PR range AND the PR title (covers both merge-commit and squash-merge flows); failures block merge via a required check
- B. Lint the PR title only — assumes squash merging
- C. Lint only merge commits / the merge result
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — commit-range lint + PR-title lint. The PR-title check was already accepted into the backlog as a nice-to-have; running both keeps enforcement correct whether maintainers merge or squash. Conductor-decided (2026-10-14); ratify at PR review.

## Q9. Which version does `/cli/latest` report once components version independently?

Context: today the endpoint embeds the API crate's `CARGO_PKG_VERSION` while serving CLI binaries — under independent versioning those numbers legitimately diverge.

- A. CLI component version — `/cli/latest` reports the latest released ox CLI version (from release metadata / the dist manifest), decoupled from the API crate's own version which stays on `system.manifest`, `system.health`, and MCP `serverInfo`
- B. Keep reporting the API crate version — document it as a deployment-server version
- C. Report both — rename the field set so API and CLI versions appear side by side
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — CLI component version. The endpoint's contract is "what CLI should I download/run"; serving the API crate's number is a latent bug independent versioning would expose. Where the number is read from (release manifest vs embedded constant) is a design decision; the requirement is the decoupling. Conductor-decided (2026-10-14); ratify at PR review.

## Q10. What shape does the wire-contract version take?

Context: `RPC_PROTOCOL_VERSION = 1` (integer), `MIN_CLI_VERSION = "0.1.0"` (semver), OpenAPI `info.version = '1'`, and duplicated MCP protocol date-lists are four disconnected version surfaces today.

- A. One SemVer wire version — a single `0.1.0` constant owned in the shared core, bumped by the same prefix rules on contract-affecting changes, reported on `system.manifest`/MCP negotiation/generated client; compatibility checks compare SemVer ranges; the integer URL path `/api/v1` stays a routing concern, not the contract version
- B. Keep integer protocol version; only components carry SemVer
- C. Two wire versions — integer protocol + SemVer contract, maintained in parallel
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — one SemVer wire version. A single SemVer aligns with the uniform-versioning intent, replaces the four divergent surfaces with one constant, and gives MIN_CLI_VERSION/self-update a sane comparison model. Exact constant name/location is a design-stage detail. Conductor-decided (2026-10-14); ratify at PR review.

## Consolidated Summary Confirmation

Summary of confirmed answers:

- Commit grammar: Conventional Commits (`type(scope)!:`) drives the hook, CI lint, bump detection, and changelog categories.
- Bump rules: 0.x-aware — breaking+feat → minor, fix/perf/revert → patch, housekeeping types → no bump; standard mapping at ≥1.0.0.
- Attribution: hybrid — conventional scope resolves to components via a registry; path globs are the fallback; the internal dependency DAG propagates hits to dependents.
- Version source of truth: each component's own manifest holds its literal version; a root `versioning.toml` registry holds metadata only, never numbers.
- Tag scheme: `<component>-v<semver>` annotated tags; `v*.*.*` triggers extended; `ox-dist` rolling tag untouched.
- Automation: one repo-owned Bun/TypeScript tool under `scripts/` owns parsing, attribution, propagation, bumping, manifest rewriting, and changelog emission.
- Hook distribution: tracked `.githooks/commit-msg` + `make setup` setting `core.hooksPath`; CI commit-lint is the non-bypassable enforcement.
- CI enforcement: lint every PR commit + the PR title; a required check blocks merge.
- `/cli/latest`: reports the ox CLI component version, decoupled from the API crate version.
- Wire contract: one SemVer version owned in shared core, reported on API/MCP/client surfaces, SemVer-compat-checked.

All ten answers are conductor decisions under the standing continuation instruction — each carries rationale and is open to reversal at PR review.

Does this all look correct before I generate the requirements artifact?

- Looks correct
- Request changes

[Answer]: Looks correct

(Recorded under the standing continuation instruction — "keep going up to the point of having tested all changes locally and having an open PR with green CI ready for review", 2026-10-14; every decided answer above is reversible at PR review.)
