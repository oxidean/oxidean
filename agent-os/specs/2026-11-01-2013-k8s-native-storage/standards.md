# Standards for Kubernetes-native storage alternatives

The following standards apply to this work.

---

## root/way-of-working

# Way of working

- **Trunk-based development** on `main`. Every change reaches `main` through
  a pull request — the `Main` ruleset blocks direct pushes (sole bypass:
  the `oxidean-release` GitHub App for stable-cut commits).
- Branches are short-lived and named `type/<slug>` (`feat/`, `fix/`,
  `chore/`, `docs/`, `ci/`), often carrying the issue number
  (`fix/large-diff-render-110`). Parallel work runs in **git worktrees**
  under sibling checkouts.
- **Squash-merge** PRs: linear `main`, one commit per PR with the `(#NNN)`
  suffix; remote branches auto-delete after merge.
- Prefer the smallest change that fits an existing pattern and existing
  workspace deps over new frameworks or libraries.
- In-repo skills under `.agents/skills/` are the documented how-we-work —
  load a matching skill before inventing a workflow.
- Scratch/temp files go under gitignored `tmp/`, never the repo root or
  source trees.
- Deploy on merge to staging; production is a manual promote of a `main`
  ancestor with the latest CI green — see `docs/TESTING.md` and the
  `production-deploy.yml` workflow.

---

## root/commits-versioning

# Commits and versioning

Every commit subject must match the enforced SemVer grammar:

```
type(optional-scope)[!]: subject
```

- Allowed `type`s live in `versioning.toml` `[grammar].allowed_types`
  (`feat`, `fix`, `chore`, `docs`, `style`, `refactor`, `perf`, `test`,
  `build`, `ci`, `revert`).
- `scope` is one token aliasing a `versioning.toml` component; omit it for
  multi-component changes (paths attribute the work).
- `!` marks breaking; `release` scope is reserved for release tooling.
- Exempt subjects: `Merge …`, `Revert "…"`, `chore(release): …`.
- A `commit-msg` hook checks locally (`make setup`); CI `commit-lint`
  checks the PR range and PR title. Subjects drive per-component SemVer
  bumps and changelogs — pick accurate types/scopes.

Each crate, Bun workspace, and the RPC wire contract carries an independent
SemVer starting at `0.1.0`; tags are `<tag_prefix><semver>`. Full grammar,
bump semantics, and release flow: [`docs/VERSIONING.md`](../../docs/VERSIONING.md)
and `.agents/skills/semver-conventions/`.

---

## root/review-practices

# Review practices

What the retired AI-DLC guard/sensor layer enforced, kept as plain rules:

- Every change lands via PR review on `main`; required status checks are the
  merge gate — when adding, renaming, or removing a CI job (or matrix
  values), update the `Main` ruleset's required-checks list in the same
  change (`docs/TESTING.md` § required status checks).
- Never commit secrets — no `.env`, tokens, or keys in the repo, CI
  config/logs, or docs examples.
- Never hand-edit generated sources (`packages/api-client`,
  `docs/openapi.yaml`) — change the Rust producers and regenerate.
- Never add AI/tool attribution (`Co-authored-by`, `Generated with`,
  `Made-with`) to commits or PRs.
- Never reintroduce `Swatinem/rust-cache` or per-job Rust toolchain installs
  in `ci.yml` — the digest-pinned `oxidean-ci:*` images are the dep cache.
- Always regenerate and commit generated output in the same change
  (`make rpc-gen`, `make openapi-gen`, and the skill/route/stack-preset/
  workflow-template sync targets — CI runs each check with
  `git diff --exit-code` semantics).
- Security-relevant operations (auth events, admin mutations, invite
  lifecycle) record an `audit::record` event — fire-and-forget; audit
  failure must never fail the operation it describes.
- Destructive RPCs (factory reset and friends) require explicit
  confirmation strings — never weaken.
- Dependency installs only via `bun install --frozen-lockfile` /
  `npm ci` / `cargo install --locked`; no update bot — freshness is manual,
  recorded debt.
- PR CI runs inside self-owned `oxidean-ci:*` images pinned to `@sha256`
  per run — the single digest-pinning point; base images and actions stay
  version-tag-pinned.

---

## root/testing

# Testing

Posture: **test-after** — implement the change, then write and run the tests
its change-type minimum requires; full CI must be green before merge. An
in-repo `tdd` skill exists for deliberate red-first work but is not the
mandated posture.

## Minimums by change type

| Change type | Minimum |
|-------------|---------|
| Pure helper | Unit test |
| UI + Query / session | happy-dom integration test |
| High-risk interactive UI (Checkbox / RadioGroup / Select / Switch / portals / `form.Subscribe`) | `*.browser.test.tsx` + `browser-coverage` manifest row |
| Auth / RPC contract | Rust integration test and/or stack e2e |
| RPC schema | `make rpc-sync-check` clean |
| Dialect-touching DB code | `dialect_*` test (CI runs `dialect_probe` only — full parity is local-capable, recorded debt) |

## Tooling and gates

- Rust: cargo-nextest (`.config/nextest.toml`); web: Vitest 5 (unit /
  integration / browser / e2e-stack projects); Playwright real Chromium;
  wiremock for outbound HTTP; protocol smokes run real `git`, `ssh`,
  `git-lfs` against a live Compose stack (fail-closed under
  `SMOKE_REQUIRE_STACK=1`).
- CI enforces a weighted composite coverage gate at floor **0.70**
  (web unit 0.25 + web integration 0.40 + e2e checklist 0.35). Rust coverage
  is collected, not gated — recorded debt.
- Route- and browser-coverage manifests are change-aware: fresh skip-only
  rows fail CI; zero `#[ignore]` / `it.skip` / `test.skip` in the tree.
- `make test` is only nextest + web unit/integration — a green local run
  proves far less than green CI; browser, stack e2e, and smokes are
  separate targets.
- CI/Docker changes are verified locally with docker + `act` before
  shipping; prefer Docker (`make up`, `make smoke`) over bare-metal
  debugging.

Details: [`docs/TESTING.md`](../../docs/TESTING.md).
