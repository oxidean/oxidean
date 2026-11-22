# WI-0004 ship record — COL-08 suggested-change blocks

## Deliverable

- Branch: `feat/col-08-suggested-changes` (based on `chore/ox-agent-0.1.25`,
  `origin/main` merged pre-push — already up to date)
- PR: https://github.com/oxidean/oxidean/pull/286 — open, **not merged**
- CodeRabbit review requested via `@coderabbitai review` comment
- CI: pending at record time (required `ci.yml` lanes + `images.yml` meta)

## What shipped

- `pull.suggestions.apply` / `pull.suggestions.apply_batch` RPCs — server-side
  ````suggestion` fence parsing, RIGHT-anchor splice at the live head tip,
  signed commit on the PR head branch (same-repo and fork heads), full
  receive-pack fan-out, `Co-authored-by` trailers for suggestion authors.
- Suggested-diff rendering in the changes tab; labeled suggestion blocks on
  all other markdown surfaces; per-suggestion commit form + shared batch
  strip with custom commit messages (GitHub defaults).
- Gates: verified session, open PR, Write on head OR base repo,
  archived/protection enforcement; stale/outdated/invalid targets rejected.

## Evidence

- `cargo nextest run -p oxidean-api-pulls`: 18/18
- `cargo nextest run -p oxidean-api --test pull_suggestions`: 7/7
- `vitest` unit+integration (touched suites): 42/42
- `make web-lint`, `make web-format-check`, `cargo fmt --check`, `cargo clippy`: clean
- `make rpc-gen` applied — `packages/api-client` regenerated

## Rollout / rollback

Additive feature — no migrations or config changes. Roll back by reverting
the PR; applied suggestions are ordinary head-branch commits.

## Human gate

`shipping → shipped` is human-gated — approval requested via durable input
request (see `.ox-agent/state/input/`).
