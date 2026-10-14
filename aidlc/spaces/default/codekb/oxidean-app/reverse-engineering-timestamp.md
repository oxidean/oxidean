# Reverse Engineering Timestamp — Oxidean monorepo

- **Reverse engineering performed**: 2026-10-14 (UTC)
- **Commit hash**: `99902328794a7fc5d6f7a3a019687500acdac443`
- **Intent**: `261014-semver-rollout` (per-component SemVer + automated
  changelog/release wiring), depth Standard
- **Scan input**:
  `aidlc/spaces/default/intents/261014-semver-rollout/inception/reverse-engineering/developer-scan.md`
  (developer-agent link 1; sole scan input, spot-checked by architect:
  `Cargo.toml` workspace members + `[workspace.package] version = "0.1.0"`,
  `oxidean-core/src/lib.rs:57` `RPC_PROTOCOL_VERSION = 1`,
  `oxidean-api/src/rpc.rs:121` `MIN_CLI_VERSION` + 283-entry `PROCEDURES`,
  `oxidean-web/src/main.rs` `OXIDEAN_API_ORIGIN` proxy, `rest/` module set,
  `apps/web` + `packages/api-client` `"version": "0.1.0"`)
- **Store state**: NO_STORE — first scan, full breadth (`paths ./`); all nine
  artifacts synthesized from this run.
- **Provenance note**: the fingerprint below is the verbatim output of
  `aidlc engine workspace codekb-scope-diff --mint --paths ./` run at
  synthesis time; working-tree HEAD at synthesis was `4d88fa76` with
  unrelated uncommitted changes (Makefile, scripts, aidlc state).

## Scope of Analysis

```yaml
scope_version: 1
kind: full
intent: 261014-semver-rollout
fingerprint: 99902328794a7fc5d6f7a3a019687500acdac443
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
    - apps/web/src/
    - apps/web/e2e/
    - crates/oxidean-api/src/actions/
    - crates/oxidean-api/src/admin/
    - crates/oxidean-api/src/auth/
    - crates/oxidean-api/src/deploy_keys/
    - crates/oxidean-api/src/email/
    - crates/oxidean-api/src/emails/
    - crates/oxidean-api/src/git/
    - crates/oxidean-api/src/gpg_keys/
    - crates/oxidean-api/src/invites/
    - crates/oxidean-api/src/issue/
    - crates/oxidean-api/src/jobs/
    - crates/oxidean-api/src/label/
    - crates/oxidean-api/src/lfs/
    - crates/oxidean-api/src/mirror/
    - crates/oxidean-api/src/notification/
    - crates/oxidean-api/src/notify/
    - crates/oxidean-api/src/oauth/
    - crates/oxidean-api/src/org/
    - crates/oxidean-api/src/packages/
    - crates/oxidean-api/src/pat/
    - crates/oxidean-api/src/protection/
    - crates/oxidean-api/src/pull/
    - crates/oxidean-api/src/release/
    - crates/oxidean-api/src/repo/
    - crates/oxidean-api/src/ssh/
    - crates/oxidean-api/src/ssh_keys/
    - crates/oxidean-api/src/templates/
    - crates/oxidean-api/src/user/
    - crates/oxidean-api/src/webhook/
    - crates/oxidean-api/tests/
    - crates/oxidean-api/assets/stack-presets/
    - crates/oxidean-db/src/
    - crates/oxidean-db/migrations/
    - crates/oxidean-cli/tests/
    - crates/oxidean-git/tests/
    - deploy/
    - docs/archive/
    - scripts/
    - .agents/skills/
    - .cursor/skills/
    - tmp/
    - var/
    - brand/
    - aidlc/
    - Makefile
    - knip.ts
```
