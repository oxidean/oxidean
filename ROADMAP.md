# Roadmap

Remaining work between the shipped v1.0 and functional parity with established code forges: the GitHub-shaped hosting surface plus the Forgejo/Gitea-class features self-host operators expect. This file is the product-level backlog; executable milestone and phase planning lives in [`.planning/`](.planning/) and follows the GSD workflow (`STATE.md`, `/gsd-new-milestone`). Items promote into phases from here.

Markers: `[ ]` not started · `[~]` in progress · `[x]` shipped on `main`. IDs (e.g. `API-03`) are stable references for issues, plans, and PRs.

## Where we are

v1.0 shipped 2026-09-19: 24 phases, 220 plans, 87/87 requirements. Git over HTTPS and SSH, code browse/blame/compare, pull requests with reviews and three merge strategies, branch protection with packaged direct-push denial, orgs and collaborator ACL, issues with labels/assignees/reactions/history, LFS, releases and repo transfer, in-repo search, in-app notifications, webhooks, Actions-compatible self-hosted runners, OCI/npm/generic packages, explore/stars/forks/watchers, topics, mirroring, admin console with setup wizard and factory reset, Compose deployment plus Railway IaC, Postgres/MySQL/SQLite. Full list: [README — What's included](README.md#whats-included); audit detail: [v1.0 milestone archive](.planning/milestones/v1.0-MILESTONE-AUDIT.md).

## Carry-overs from v1.0

Deferred items recorded at milestone close. Finish these first; several are cheap and unblock later sections.

- [ ] **DEBT-01** Wire `make smoke-protection` into `scripts/ci-smoke-protocol.sh` and the CI smoke job. The ORG-06 direct-push denial guard is green locally but absent from CI.
- [ ] **DEBT-02** Raise the coverage floor from 0.65 toward ~0.70 and re-enable `cargo-llvm-cov` collection in CI (WINDOWS entries 53–54 open).
- [ ] **DEBT-03** Sitewide GlobalSearch depth. Header search routes to `/explore?q=` today; extend to users, orgs, commits, PRs, and code.
- [ ] **DEBT-04** `issue_comment` webhook event. The notification fan-out exists; webhook emission does not.
- [ ] **DEBT-05** Checks / commit-status viewer tab on PR pages. Status contexts already gate merges; there is no dedicated UI to inspect them.
- [ ] **DEBT-06** Watch repositories and follow users, with a per-repo notification matrix.
- [ ] **DEBT-07** Live `railway config apply` operator path. Kept human-verify in v1; CI must not hold tokens.
- [ ] **DEBT-08** Nyquist validation backfill for the not-validated phases (10–13, 16–18, 21, 22.1; 11.1 missing entirely).
- [ ] **DEBT-09** Backfill the AUTH-07a/07b requirement rows in the phase 06 VERIFICATION table (product shipped; table orphaned).
- [~] **DEBT-10** Admin user management in the console: list, ban, delete, role change, access view, invites. First cut in flight, not yet on `main`.
- [ ] **DEBT-11** Deeper live smokes: git-over-SSH `ls-remote`/push depth (phase 09 caveat) and releases/rename browser UAT (phase 15 caveat).

## API and integrations

The forge is only as useful as what third-party tools can drive. Today the typed `/api/rpc` surface is session-cookie only and PATs authenticate git, LFS, and registries but not RPC — the single largest integration gap.

- [ ] **API-01** Public REST API with OpenAPI spec covering the core surface: repos, issues, PRs, comments, releases, orgs, users, admin. Companion to the RPC layer, not a replacement.
- [ ] **API-02** Token-authenticated API access: extend PAT scopes (or issue OAuth bearer tokens) so non-browser clients can call the API. Includes deciding whether `/api/rpc` accepts tokens or the REST surface is the only programmatic path.
- [ ] **API-03** OAuth apps: instance acts as an OAuth2/OIDC provider so external tools authenticate users ("sign in with Oxidean") and act on their behalf. Tracked as PLAT-V2-02 alongside fine-grained PAT depth.
- [ ] **API-04** Broader webhook events beyond `push`, `pull_request`, `issues`, `ping`, `*`: `issue_comment` (DEBT-04), `release`, `star`, `fork`, `create`/`delete` refs, workflow run status, package publish.
- [ ] **API-05** Atom/RSS feeds for repo activity, releases, and user activity.
- [ ] **API-06** GitHub-compatible subset for common tooling (commit statuses API shape, PR refs `refs/pull/N/head`, known webhook payload conventions) so existing CI/deploy bots work unchanged where practical.

## Git and code surface

- [ ] **GIT-21** Web file editing: create, edit, rename, delete, and upload files with a commit from the browser, signed by the existing web-flow key. Includes new-branch-with-PR flow.
- [ ] **GIT-22** Repository archive flag: read-only mode that blocks push, issues, and PRs while keeping everything browsable and clonable.
- [ ] **GIT-23** Protected tags and tag rulesets (branch protection covers branches only today).
- [ ] **GIT-24** "Require signed commits" protection option. SSH/GPG signature verification is displayed on commits; enforcement is not a protection knob yet.
- [ ] **GIT-25** Deploy keys: per-repo SSH keys with read or read/write scope, distinct from account keys.
- [ ] **GIT-26** Sync fork and update-PR-branch: bring a fork or PR head up to date with the base branch from the UI/API.
- [ ] **GIT-27** Repository size quotas for git objects. LFS and package quotas exist; the bare repo itself is unbounded.
- [ ] **GIT-28** Repo insights: contributors, commit activity, and fork-network views on top of the existing repo activity feed.
- [ ] **GIT-29** Git protocol surface audit: confirm protocol v2, partial clone/filter, and shallow clone behavior on both transports, then document or fix.

## Issues, PRs, and collaboration

The v1 loop (open, comment, review, merge, close) works. Parity is about the planning and review depth around it.

- [ ] **COL-01** Milestones: group issues/PRs by release goal with progress and due dates.
- [ ] **COL-02** Issue and PR templates from `ISSUE_TEMPLATE/` and `PULL_REQUEST_TEMPLATE` in-repo files, including multi-template chooser.
- [ ] **COL-03** Comment attachments: image and file uploads in issue/PR/release discussions.
- [ ] **COL-04** Pinned issues/PRs per repo and pinned repos on user/org profiles.
- [ ] **COL-05** Org teams: named groups with their own repo access lists. Orgs are flat `owner`/`admin`/`member` today.
- [ ] **COL-06** CODEOWNERS-enforced review requests (COLLAB-V2-03).
- [ ] **COL-07** Merge queue and auto-merge-when-green (COLLAB-V2-04).
- [ ] **COL-08** Suggested-change code blocks in review comments.
- [ ] **COL-09** Discussions and gists/snippets as separate sharing surfaces.
- [ ] **COL-10** Project boards / kanban scoped to repo or org (COLLAB-V2-01).
- [ ] **COL-11** Wiki per repository (COLLAB-V2-02).
- [ ] **COL-12** Contribution graph / profile activity and profile README support.

## Actions (CI)

v1 runs push, pull_request, and manual dispatch on self-hosted runners. Parity needs the scheduling and artifact surfaces real CI depends on.

- [ ] **CI-01** More triggers: `schedule` (cron), tag/release events, `create`/`delete`, `workflow_run`, plus branch/tag/path filters on push and pull_request.
- [ ] **CI-02** Job graph: `needs` ordering, matrix strategies, `concurrency` groups with cancel-in-progress, reusable workflows (`workflow_call`).
- [ ] **CI-03** Artifacts: upload/download between jobs and from the UI, with retention policy.
- [ ] **CI-04** Dependency cache equivalent to `actions/cache` on the runner protocol.
- [ ] **CI-05** Environments and deployment protection rules.
- [ ] **CI-06** Runner groups and org-level runner registration; today only sys-admin registration tokens exist.
- [ ] **CI-07** Workflow status badges endpoint for README embedding.
- [ ] **CI-08** Re-run failed jobs and step-level debug logging controls.
- [ ] **CI-09** Job `id-token`/OIDC for keyless cloud auth from workflows.

## Packages and registries

- [ ] **PKG-06** Additional ecosystems beyond OCI/npm/generic: PyPI, Maven, Cargo, NuGet, Helm charts, Go modules — the common Forgejo-class subset.
- [ ] **PKG-07** Retention and cleanup policies per package/owner.
- [ ] **PKG-08** Optional upstream pull-through caching/proxying for supported formats.

## Search and discovery

- [ ] **SRCH-01** Instance-wide cross-repo code search backed by an index (COLLAB-V2-05). In-repo search ships today; global code search does not.
- [ ] **SRCH-02** Search qualifiers and filters (org:, language:, path:) consistent across web UI and API.
- [ ] **SRCH-03** Topic and org browsing pages on `/explore` (topics exist per-repo; discovery does not).

## Notifications and email

- [ ] **NOT-01** Outbound email for watched/assigned/mentioned issue and PR activity. Notifications are in-app only today.
- [ ] **NOT-02** Email digests (COLLAB-V2-06).
- [ ] **NOT-03** Reply-by-email: inbound replies append to the issue/PR thread.
- [ ] **NOT-04** Notification filters: watching matrix (DEBT-06), per-repo subscriptions, mute.

## Auth, security, and moderation

- [ ] **SEC-01** OAuth login for GitHub and/or Google (AUTH-V2-01). OIDC and WorkOS exist; the common consumer providers do not.
- [ ] **SEC-02** Two-factor authentication: TOTP with recovery codes, WebAuthn/passkeys (AUTH-V2-02).
- [ ] **SEC-03** LDAP auth provider for self-host parity.
- [ ] **SEC-04** Session management UI: list active sessions, revoke individually. `auth.logout_all` exists; per-device control does not.
- [ ] **SEC-05** Signup and abuse controls: optional CAPTCHA, broader rate limiting, invite codes for closed instances.
- [ ] **SEC-06** Audit log: admin, auth, repo-admin, and permission-changing events, queryable by sys-admin.
- [ ] **SEC-07** User block/report and an admin moderation queue.
- [ ] **SEC-08** Security advisories, dependency graph, and vulnerability scanning — staged last; pairs with CI and packages work.

## Migration and interoperability

- [ ] **MIG-01** One-shot repository import from GitHub, Gitea/Forgejo, and GitLab: git data plus issues, PRs, releases, LFS. Continuous mirrors exist; a full migrator does not.
- [ ] **MIG-02** Instance backup/export and documented restore: DB dump plus repos, LFS, packages, uploads. Factory reset exists; no export path does.
- [ ] **MIG-03** Per-repo export bundle for portability and archival.

## Admin and operations

- [ ] **OPS-01** Metrics endpoint (Prometheus) and observability docs.
- [ ] **OPS-02** S3-compatible object storage for LFS, packages, and uploads; filesystem is the only backend today.
- [ ] **OPS-03** HA / horizontal scaling guidance: shared git storage, DB, object store, runner fleet.
- [ ] **OPS-04** UI internationalization framework; the product is English-only.
- [ ] **OPS-05** Admin console depth beyond DEBT-10: repo listing/search, runner detail, system info, broadcast notices.

## Exploratory / later

Deliberately parked. Revisit after the sections above land.

- [ ] **XPL-01** Federation / ActivityPub (PLAT-V2-01).
- [ ] **XPL-02** Official mobile apps (PLAT-V2-03; out of scope for v1 and still uncommitted).
- [ ] **XPL-03** Managed runner minutes on Oxidean Cloud. v1 is self-hosted runners only by design.
- [ ] **XPL-04** Sponsors/marketplace surfaces.

## Non-goals

Carried from [.planning/PROJECT.md](.planning/PROJECT.md) so this roadmap stays honest about what parity does not mean:

- Replacing git with a custom VCS; real git clients must keep working.
- Forking Gitea/Forgejo as the product identity.
- Separate cloud-only and self-host-only feature sets; one product, one release train.
- Non-Docker runtime targets for the forge app.

## Suggested order

1. **DEBT-\*** — close v1.0 residue; several items (DEBT-01, DEBT-04, DEBT-05) are prerequisites for later sections.
2. **API-01..03** — the integration surface is the biggest parity blocker; it also unlocks MIG-01 tooling.
3. **GIT-21..25** — web editing, archive, protected tags, deploy keys: the most-felt daily gaps for repo admins.
4. **COL-01..08** — collaboration depth (milestones, templates, attachments, CODEOWNERS, merge queue).
5. **CI-01..05** — scheduling, job graph, artifacts, cache, environments.
6. **SEC-01..04** — OAuth login, 2FA, LDAP, session controls.
7. **SRCH, NOT, PKG, OPS** — breadth tracks, parallelizable once the API surface exists.
8. **MIG-\*** — importers gain the most from the REST API landing first.
9. **XPL-\*** — last, on explicit decision.

---
*Created 2026-10-01. Update status markers as items land; do not delete entries — mark them `[x]` and link the PR.*
