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
