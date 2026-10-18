# Reverse Engineering Timestamp — Oxidean monorepo

- **Reverse engineering performed**: 2026-10-18 (UTC)
- **Commit hash**: `381210af6ec4f6b9e5e384e3a36811e1e4877479`
- **Branch**: `feat/agentic-vcs-protocol` (git worktree
  `oxidean-app-agentic-vcs`)
- **Intent**: `261018-agentic-vcs-protocol` (side-by-side non-git
  agentic-VCS protocol in a new dedicated Rust crate), depth Standard,
  scope `classic`, Brownfield workspace
- **Scan input**:
  `aidlc/spaces/default/intents/261018-agentic-vcs-protocol/inception/reverse-engineering/developer-scan.md`
  (developer-agent link 1; sole scan input, spot-checked by architect:
  `git worktree list` branch/HEAD match, `.tsrx` counts (226 total /
  142 components), `crates/oxidean-db/migrations/postgres/` = 50 numbered
  SQL files, `crates/oxidean-api/tests/` = 106 `.rs` files at glob —
  scan's "107" retained as the scan's count incl. support module)
- **Store state**: STALE → **full rescan** (wholesale replace of all nine
  artifacts); prior store at `aidlc/spaces/default/codekb/oxidean-app/`
  recorded intent `261014-semver-rollout` at commit `99902328` and is
  superseded — per-component SemVer has since shipped (`versioning.toml`,
  `commit-msg` hook, `release.yml`), and procedure/tool/route counts are
  updated throughout.
- **Fingerprint provenance**: the fingerprint below is the verbatim output
  of `aidlc engine workspace codekb-scope-diff --mint --paths ./` run from
  the worktree root at synthesis time; working tree carried the active
  intent's uncommitted `aidlc/` record files only.

## Scope of Analysis

```yaml
scope_version: 1
kind: full
intent: 261018-agentic-vcs-protocol
fingerprint: 1c07332f5b68572a57b04aeddb9565521e94a0d8
analyzed:
  paths:
    - ./
  components:
    - oxidean-api
    - oxidean-cli
    - oxidean-core
    - oxidean-db
    - oxidean-git
    - oxidean-runner
    - oxidean-web
    - "@oxidean/web"
    - "@oxidean/api-client"
    - oxidean
shallow:
  paths:
    - apps/web/src/components/
    - apps/web/src/lib/
    - apps/web/src/routes/
    - apps/web/public/
    - crates/oxidean-api/src/actions/
    - crates/oxidean-api/src/admin/
    - crates/oxidean-api/src/auth/
    - crates/oxidean-api/src/deploy_keys/
    - crates/oxidean-api/src/emails/
    - crates/oxidean-api/src/gpg_keys/
    - crates/oxidean-api/src/invites/
    - crates/oxidean-api/src/issue/
    - crates/oxidean-api/src/label/
    - crates/oxidean-api/src/mirror/
    - crates/oxidean-api/src/oauth/
    - crates/oxidean-api/src/org/
    - crates/oxidean-api/src/packages/
    - crates/oxidean-api/src/protection/
    - crates/oxidean-api/src/pull/
    - crates/oxidean-api/src/release/
    - crates/oxidean-api/src/search/
    - crates/oxidean-api/src/ssh_keys/
    - crates/oxidean-api/src/templates/
    - crates/oxidean-api/src/user/
    - crates/oxidean-api/src/webhook/
    - crates/oxidean-db/src/
    - crates/oxidean-db/migrations/
    - docs/archive/
    - docs/guides/
    - .agents/
    - .claude/
    - .cursor/
    - .github/ISSUE_TEMPLATE/
    - brand/
    - tmp/
    - bun.lock
    - Cargo.lock
    - skills-lock.json
```
