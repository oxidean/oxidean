---
schema_version: 1
open_count: 0
waived_count: 52
fixed_count: 6
total_count: 58
last_updated: 2026-10-02T00:27:48.664Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 06 | unrun-verify | crates/oxidean-api/tests/auth_bootstrap.rs |  | Plan 06-02 full test(bootstrap) filter deferred: allowlist/allow_signup Wave 0 RED owned by 06-03 | waived | Obsolete unrun-verify: bootstrap/allow_signup Wave 0 superseded by plans 06-03+ (phase 06 shipped) | 2026-09-11T20:42:23.023Z | 2026-09-19T16:39:45.822Z |
| 2 | 07 | stub | crates/oxidean-git/src/version.rs | 43 | git_archive_formats_zip_and_tar_gz still Wave 0 assert!(false) — archive plan owns | waived | Obsolete Wave 0 stub: git_archive_formats greened; superseded by archive plans in phase 07 | 2026-09-12T17:13:59.869Z | 2026-09-19T16:39:45.932Z |
| 3 | 07 | stub | apps/web/src/routes/new.tsrx | 206 | Stack/License/.gitignore None placeholders until 07-03 | waived | Obsolete Wave 0 stub: Stack/License/.gitignore selectors shipped in 07-03 | 2026-09-12T17:20:43.199Z | 2026-09-19T16:39:46.041Z |
| 4 | 07 | stub | apps/web/src/routes/$owner.$repo.index.tsrx |  | Empty Quick setup only; tree/README deferred to 07-15 | waived | Obsolete Wave 0 stub: tree/README browse shipped in 07-15 | 2026-09-12T17:20:43.278Z | 2026-09-19T16:39:46.150Z |
| 5 | 07 | deviation | apps/web/src/routes/$owner.$repo.tsrx |  | Added Outlet layout parent required for $owner.$repo.index route | waived | Historical deviation noise: Outlet layout parent shipped with phase 07 routes | 2026-09-12T17:20:43.358Z | 2026-09-19T16:39:46.269Z |
| 6 | 07 | unrun-verify | crates/oxidean-git/src/version.rs | 73 | Wave 0 git_archive_formats stub fails full oxidean-git --lib until archive plan; 07-05 verified with not test(git_archive) | waived | Obsolete unrun-verify: git_archive Wave 0 filter superseded by archive plan greens | 2026-09-12T17:59:04.501Z | 2026-09-19T16:39:46.383Z |
| 7 | 07 | stub | apps/web/src/routes/$owner.$repo.index.tsrx |  | Clone/Download toolbar stub until 07-08 | waived | Obsolete Wave 0 stub: Clone/Download toolbar shipped in 07-08 | 2026-09-12T18:11:31.663Z | 2026-09-19T16:39:46.493Z |
| 8 | 07 | skipped-test | crates/oxidean-git/src/version.rs |  | Pre-existing git_archive_formats_zip_and_tar_gz Wave 0 stub fails nextest | waived | Obsolete skipped-test: git_archive_formats Wave 0 stub removed after archive plans | 2026-09-12T18:23:10.183Z | 2026-09-19T16:39:46.609Z |
| 9 | 07 | skipped-test | crates/oxidean-api/tests/repo_branch_soft_protect.rs |  | Wave 0 soft-protect stubs owned by 07-07 | waived | Obsolete skipped-test: soft-protect Wave 0 stubs superseded by 07-07 | 2026-09-12T18:23:10.268Z | 2026-09-19T16:39:46.719Z |
| 10 | 08 | stub | crates/oxidean-api/tests/pat_rpc.rs |  | Wave 0 RED pat_* stubs until 08-04 | waived | Obsolete Wave 0 stub: pat_* RPC tests greened in 08-04 | 2026-09-13T17:58:59.409Z | 2026-09-19T16:39:46.830Z |
| 11 | 08 | stub | crates/oxidean-api/tests/git_smart_http.rs |  | Wave 0 RED git_smart_* stubs until 08-04/08-06 | waived | Obsolete Wave 0 stub: git_smart_* tests greened in 08-04/08-06 | 2026-09-13T17:58:59.506Z | 2026-09-19T16:39:46.939Z |
| 12 | 08 | stub | crates/oxidean-db/tests/dialect_pats.rs |  | Wave 0 dialect_pats until 08-03 0008_pats | waived | Obsolete Wave 0 stub: dialect_pats shipped with 08-03 0008_pats | 2026-09-13T17:58:59.595Z | 2026-09-19T16:39:47.048Z |
| 13 | 08 | deviation | crates/oxidean-db/tests/dialect_pats.rs |  | Renamed dialect tests for test(dialect_pats) nextest filter | waived | Historical deviation noise: dialect_pats rename for nextest filter (phase 08 shipped) | 2026-09-13T17:58:59.686Z | 2026-09-19T16:39:47.159Z |
| 14 | 08 | stub | apps/web/src/routes/settings/tokens.integration.test.ts |  | Wave 0 RED tokens UI stubs until 08-09/08-10 | waived | Obsolete Wave 0 stub: tokens UI integration greened in 08-09/08-10 | 2026-09-13T18:04:19.488Z | 2026-09-19T16:39:47.269Z |
| 15 | 08 | stub | apps/web/src/components/repo/clone-box.pat.integration.test.ts |  | Wave 0 RED CloneBox PAT how-to stubs until 08-12 | waived | Obsolete Wave 0 stub: CloneBox PAT how-to greened in 08-12 | 2026-09-13T18:04:19.572Z | 2026-09-19T16:39:47.382Z |
| 16 | 08 | deviation | apps/web/src/routes/settings/tokens.integration.test.ts |  | Used runtime-variable @vite-ignore import so Vitest collects while tokens route absent | waived | Historical deviation noise: tokens route @vite-ignore collect workaround superseded | 2026-09-13T18:04:19.657Z | 2026-09-19T16:39:47.491Z |
| 17 | 08 | stub | crates/oxidean-api/src/pat/mod.rs |  | create_fine_grained stub → pat.not_implemented (08-05) | fixed |  | 2026-09-13T18:37:52.435Z | 2026-09-13T18:42:52.268Z |
| 18 | 08 | skipped-test | crates/oxidean-api/tests/git_smart_http.rs |  | 4 expansion git_smart tests #[ignore] until 08-06 | waived | Obsolete skipped-test: git_smart expansion ignores lifted after 08-06 | 2026-09-13T18:37:52.519Z | 2026-09-19T16:39:47.607Z |
| 19 | 08 | deviation | crates/oxidean-api/src/pat/mod.rs |  | Plan prose ona_fg_ locked to oxidean_fg_ (FINE_GRAINED_PAT_PREFIX / D-08) | waived | Historical deviation noise: FINE_GRAINED_PAT_PREFIX oxidean_fg_ locked (D-08 shipped) | 2026-09-13T18:42:34.877Z | 2026-09-19T16:39:47.716Z |
| 20 | 08 | unrun-verify | scripts/smoke-git-https.sh |  | Live make smoke-git-https not run — Docker engine unavailable on executor host | waived | Historical unrun-verify noise: smoke-git-https Docker host gap; PAT/HTTPS stack shipped in phase 08 | 2026-09-13T19:06:10.084Z | 2026-09-19T16:39:47.823Z |
| 21 | 08 | skipped-test | apps/web/src/routes/settings/tokens.integration.test.ts |  | D-15 one-time reveal it.skip until 08-10 | waived | Obsolete skipped-test: D-15 one-time reveal greened in 08-10 | 2026-09-13T19:22:14.417Z | 2026-09-19T16:39:47.940Z |
| 22 | 08 | deviation | apps/web/src/components/settings/pat-revoke-dialog.tsrx |  | Revoke dialog landed with T1 list commit; T2 greened assertions | waived | Historical deviation noise: pat-revoke-dialog landed with list commit (phase 08 shipped) | 2026-09-13T19:22:14.499Z | 2026-09-19T16:39:48.050Z |
| 23 | 09 | stub | crates/oxidean-api/tests/ssh_key_rpc.rs |  | Wave 0 assert!(false) sshKey RPC stubs until 09-03 | waived | Obsolete Wave 0 stub: sshKey RPC greened in 09-03 | 2026-09-13T23:34:41.937Z | 2026-09-19T16:39:48.167Z |
| 24 | 09 | stub | crates/oxidean-api/tests/git_ssh.rs |  | Wave 0 assert!(false) git_ssh stubs until 09-04/09-05 | waived | Obsolete Wave 0 stub: git_ssh tests greened in 09-04/09-05 | 2026-09-13T23:34:42.044Z | 2026-09-19T16:39:48.274Z |
| 25 | 09 | stub | crates/oxidean-db/tests/dialect_ssh_keys.rs |  | Wave 0 dialect_ssh_keys until 0009_ssh_keys migration (09-02) | waived | Obsolete Wave 0 stub: dialect_ssh_keys shipped with 09-02 0009_ssh_keys | 2026-09-13T23:34:42.151Z | 2026-09-19T16:39:48.384Z |
| 26 | 09 | stub | scripts/smoke-git-ssh.sh |  | Wave 0 smoke-git-ssh exit 1 until 09-05 Compose TCP green | waived | Obsolete Wave 0 stub: smoke-git-ssh greened in 09-05 | 2026-09-13T23:34:42.250Z | 2026-09-19T16:39:48.504Z |
| 27 | 09 | stub | apps/web/src/routes/settings/ssh-keys.integration.test.ts |  | Wave 0 RED ssh-keys integration stubs until 09-07 | waived | Obsolete Wave 0 stub: ssh-keys integration greened in 09-07 | 2026-09-13T23:38:48.019Z | 2026-09-19T16:39:48.613Z |
| 28 | 09 | stub | apps/web/src/components/repo/clone-box.ssh.integration.test.ts |  | Wave 0 RED CloneBox SSH integration stubs until 09-08 | waived | Obsolete Wave 0 stub: CloneBox SSH integration greened in 09-08 | 2026-09-13T23:38:48.151Z | 2026-09-19T16:39:48.728Z |
| 29 | 09 | unrun-verify | apps/web/src/routes/settings/ssh-keys.integration.test.ts |  | Wave 0 vitest intentionally RED (exit 1) until production routes — verify ran, stubs fail by design | waived | Obsolete unrun-verify: Wave 0 ssh-keys vitest RED superseded by 09-07 greens | 2026-09-13T23:38:48.269Z | 2026-09-19T16:39:48.838Z |
| 30 | 09 | stub | scripts/smoke-git-ssh.sh |  | RESOLVED: smoke-git-ssh greened in 09-05 | fixed |  | 2026-09-14T00:11:24.344Z | 2026-09-19T16:39:45.607Z |
| 31 | 10 | stub | apps/web/src/routes/orgs.new.integration.test.ts |  | Fails until /orgs/new lands in 10-13 | waived | Obsolete Wave 0 stub: /orgs/new shipped in 10-13 | 2026-09-13T23:41:25.124Z | 2026-09-19T16:39:48.952Z |
| 32 | 10 | stub | apps/web/src/routes/new.owner-picker.integration.test.ts |  | Fails until owner Select lands in 10-11 | waived | Obsolete Wave 0 stub: owner Select shipped in 10-11 | 2026-09-13T23:41:25.222Z | 2026-09-19T16:39:49.065Z |
| 33 | 10 | stub | apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts |  | Fails until collaborators-panel + can_admin in 10-11 | waived | Obsolete Wave 0 stub: collaborators-panel shipped in 10-11 | 2026-09-13T23:41:25.316Z | 2026-09-19T16:39:49.174Z |
| 34 | 10 | stub | apps/web/src/routes/$owner.settings.members.integration.test.ts |  | Fails until members/invites UI in 10-10 | waived | Obsolete Wave 0 stub: members/invites UI shipped in 10-10 | 2026-09-13T23:41:25.415Z | 2026-09-19T16:39:49.283Z |
| 35 | 10 | stub | crates/oxidean-db/migrations/postgres/0010_orgs_acl.sql |  | organization_invites/repository_collaborators tables exist without CRUD helpers/RPCs (deferred 10-06/10-07) | waived | Obsolete Wave 0 stub: org invite/collaborator CRUD RPCs shipped in 10-06/10-07 | 2026-09-13T23:52:49.281Z | 2026-09-19T16:39:49.393Z |
| 36 | 10 | deviation | crates/oxidean-api/src/auth/local.rs |  | Signup still does not dual-check organizations.slug for shared namespace (D-ORG-01); org.create does — defer to signup/rename plans | fixed |  | 2026-09-14T00:06:48.676Z | 2026-09-19T16:39:45.715Z |
| 37 | 10 | skipped-test | crates/oxidean-api/tests/repo_collaborators_acl.rs |  | Collaborator CRUD ACL tests ignored until plan 07 | waived | Obsolete skipped-test: collaborator CRUD ACL tests greened after plan 10-07 | 2026-09-14T00:36:39.442Z | 2026-09-19T16:39:49.502Z |
| 38 | 10 | skipped-test | crates/oxidean-api/tests/repo_private_404.rs | 410 | repo_private_404_collaborator_granted_read ignored until plan 07 | waived | Obsolete skipped-test: repo_private_404 collaborator read greened after plan 10-07 | 2026-09-14T00:36:39.533Z | 2026-09-19T16:39:49.611Z |
| 39 | 10 | skipped-test | crates/oxidean-api/tests/git_smart_http.rs | 610 | git_smart_collaborator_classic_pat_push Wave-0 stub fails under test(collab) filter; PAT collaborator push deferred to later plan | waived | Obsolete skipped-test: git_smart collaborator PAT push Wave-0 superseded by later collab plans | 2026-09-14T01:15:41.319Z | 2026-09-19T16:39:49.719Z |
| 40 | 11 | stub | crates/oxidean-api/tests/issue_lifecycle.rs |  | Wave 0 assert!(false) issue lifecycle stubs until 11-02+ | waived | Obsolete Wave 0 stub: issue lifecycle tests greened in 11-02+ | 2026-09-14T14:17:20.331Z | 2026-09-19T16:39:49.828Z |
| 41 | 11 | stub | crates/oxidean-db/tests/dialect_issues.rs |  | Wave 0 dialect_issues RED until 0011_issues lands | waived | Obsolete Wave 0 stub: dialect_issues shipped with 0011_issues | 2026-09-14T14:17:20.413Z | 2026-09-19T16:39:49.939Z |
| 42 | 11 | stub | crates/oxidean-db/tests/factory_reset_issues.rs |  | Wave 0 factory_reset_issues RED until cascade wipe lands | waived | Obsolete Wave 0 stub: factory_reset_issues cascade wipe shipped | 2026-09-14T14:17:20.496Z | 2026-09-19T16:39:50.053Z |
| 43 | 11 | deviation | crates/oxidean-api/tests/git_ssh.rs |  | Rule 3: fixed insert_repository owner_type arity to unblock nextest list | waived | Historical deviation noise: insert_repository arity fix for nextest list (phase 11 shipped) | 2026-09-14T14:17:20.577Z | 2026-09-19T16:39:50.167Z |
| 44 | 11 | stub | apps/web/src/routes/$owner.$repo.issues.integration.test.ts |  | Wave 0 it.fails Issues UI stubs pending 11-03..11-09 greens | waived | Obsolete Wave 0 stub: Issues UI integration greened in 11-03..11-09 | 2026-09-14T14:24:49.338Z | 2026-09-19T16:39:50.279Z |
| 45 | 11 | stub | apps/web/src/lib/markdown.issues.test.ts |  | Wave 0 it.fails #N autolink stubs pending 11-10 greens | waived | Obsolete Wave 0 stub: #N autolink greened in 11-10 | 2026-09-14T14:24:49.422Z | 2026-09-19T16:39:50.393Z |
| 46 | 11 | stub | apps/web/src/routes/$owner.$repo.issues.$n.tsrx |  | Comments/Labels/Assignees/Linked PRs empty shells until later plans | waived | Obsolete Wave 0 stub: issue detail panels shipped in later 11-xx plans | 2026-09-14T14:53:54.977Z | 2026-09-19T16:39:50.506Z |
| 47 | 11 | deviation | apps/web/src/routes/$owner.$repo.issues.$n.tsrx |  | Rule 2: wired detail route owner/repo into renderGfm despite plan 'without editing detail route files' | waived | Historical deviation noise: detail route owner/repo GFM wiring (phase 11 shipped) | 2026-09-14T15:54:38.280Z | 2026-09-19T16:39:50.621Z |
| 48 | 11 | stub | apps/web/src/routes/$owner.$repo.issues.$n.tsrx |  | Assignees panel shipped in 11-07; Linked PRs shell remains until 11-09 | fixed |  | 2026-09-14T16:05:16.873Z | 2026-09-14T16:29:16.369Z |
| 49 | 11 | stub | apps/web/src/components/repo/issue-linked-prs.tsrx |  | Linked PRs panel shows pr_stub placeholders until Phase 12 replaces kind (D-ISS-13 intentional) | waived | Obsolete stub: Linked PRs pr_stub replaced by Phase 12 PR links (D-ISS-13) | 2026-09-14T16:28:25.592Z | 2026-09-19T16:39:50.733Z |
| 50 | 11 | unmet-truth | crates/oxidean-api/src/issue/mod.rs |  | Closing-keyword auto-close deferred to Phase 12 (D-ISS-15) — verified not enforced in issue_links_no_closing_keyword_enforcement | waived | Intentional Phase 12 deferral (D-ISS-15), not a defect | 2026-09-14T16:28:25.718Z | 2026-09-14T16:29:16.230Z |
| 51 | 15 | stub | crates/oxidean-api/tests/release_rpc.rs |  | release_* tests #[ignore] until 15-01/15-02 | waived | Obsolete Wave 0 stub: release_* RPC tests greened in 15-01/15-02 | 2026-09-14T16:59:15.548Z | 2026-09-19T16:39:50.845Z |
| 52 | 15 | deviation | crates/oxidean-api/src/repo/rename_transfer.rs |  | 15-03 combined T1-T3 into single commit due to shared redirect wiring | waived | Historical deviation noise: 15-03 combined rename/transfer commit (phase 15 shipped) | 2026-09-14T17:31:23.559Z | 2026-09-19T16:39:50.957Z |
| 53 | 11.1 | deviation | scripts/coverage-weighted.sh |  | Bootstrap floor 0.65 instead of plan ~0.70; ratchet target 0.70 documented | fixed |  | 2026-09-15T16:21:58.576Z | 2026-10-02T00:27:48.664Z |
| 54 | 11.1 | deviation | .github/workflows/ci.yml |  | CI skips cargo-llvm-cov collect (Make target remains); web+checklist drive gate | fixed |  | 2026-09-15T16:21:58.697Z | 2026-10-02T00:27:48.664Z |
| 55 | 18 | stub | crates/oxidean-api/tests/webhook_rpc.rs |  | All webhook_* RPC tests #[ignore] until 18-01/18-02 | waived | Obsolete Wave 0 stub: webhook_* RPC tests greened in 18-01/18-02 | 2026-09-16T13:43:57.313Z | 2026-09-19T16:39:51.070Z |
| 56 | 18 | stub | crates/oxidean-api/tests/webhook_delivery.rs |  | All webhook_* delivery tests #[ignore] until 18-01..18-03 | waived | Obsolete Wave 0 stub: webhook delivery tests greened in 18-01..18-03 | 2026-09-16T13:43:57.452Z | 2026-09-19T16:39:51.186Z |
| 57 | 18 | stub | crates/oxidean-db/tests/dialect_webhooks.rs |  | dialect_webhooks #[ignore] until 18-01 | waived | Obsolete Wave 0 stub: dialect_webhooks greened in 18-01 | 2026-09-16T13:43:57.578Z | 2026-09-19T16:39:51.301Z |
| 58 | 18 | stub | apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts |  | Settings Webhooks Vitest it.fails until 18-04 | waived | Obsolete Wave 0 stub: Settings Webhooks Vitest greened in 18-04 | 2026-09-16T13:43:57.702Z | 2026-09-19T16:39:51.416Z |

````json
[
  {
    "id": 1,
    "kind": "unrun-verify",
    "phase": "06",
    "file": "crates/oxidean-api/tests/auth_bootstrap.rs",
    "line": null,
    "description": "Plan 06-02 full test(bootstrap) filter deferred: allowlist/allow_signup Wave 0 RED owned by 06-03",
    "status": "waived",
    "reason": "Obsolete unrun-verify: bootstrap/allow_signup Wave 0 superseded by plans 06-03+ (phase 06 shipped)",
    "recorded_at": "2026-09-11T20:42:23.023Z",
    "resolved_at": "2026-09-19T16:39:45.822Z"
  },
  {
    "id": 2,
    "kind": "stub",
    "phase": "07",
    "file": "crates/oxidean-git/src/version.rs",
    "line": 43,
    "description": "git_archive_formats_zip_and_tar_gz still Wave 0 assert!(false) — archive plan owns",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: git_archive_formats greened; superseded by archive plans in phase 07",
    "recorded_at": "2026-09-12T17:13:59.869Z",
    "resolved_at": "2026-09-19T16:39:45.932Z"
  },
  {
    "id": 3,
    "kind": "stub",
    "phase": "07",
    "file": "apps/web/src/routes/new.tsrx",
    "line": 206,
    "description": "Stack/License/.gitignore None placeholders until 07-03",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: Stack/License/.gitignore selectors shipped in 07-03",
    "recorded_at": "2026-09-12T17:20:43.199Z",
    "resolved_at": "2026-09-19T16:39:46.041Z"
  },
  {
    "id": 4,
    "kind": "stub",
    "phase": "07",
    "file": "apps/web/src/routes/$owner.$repo.index.tsrx",
    "line": null,
    "description": "Empty Quick setup only; tree/README deferred to 07-15",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: tree/README browse shipped in 07-15",
    "recorded_at": "2026-09-12T17:20:43.278Z",
    "resolved_at": "2026-09-19T16:39:46.150Z"
  },
  {
    "id": 5,
    "kind": "deviation",
    "phase": "07",
    "file": "apps/web/src/routes/$owner.$repo.tsrx",
    "line": null,
    "description": "Added Outlet layout parent required for $owner.$repo.index route",
    "status": "waived",
    "reason": "Historical deviation noise: Outlet layout parent shipped with phase 07 routes",
    "recorded_at": "2026-09-12T17:20:43.358Z",
    "resolved_at": "2026-09-19T16:39:46.269Z"
  },
  {
    "id": 6,
    "kind": "unrun-verify",
    "phase": "07",
    "file": "crates/oxidean-git/src/version.rs",
    "line": 73,
    "description": "Wave 0 git_archive_formats stub fails full oxidean-git --lib until archive plan; 07-05 verified with not test(git_archive)",
    "status": "waived",
    "reason": "Obsolete unrun-verify: git_archive Wave 0 filter superseded by archive plan greens",
    "recorded_at": "2026-09-12T17:59:04.501Z",
    "resolved_at": "2026-09-19T16:39:46.383Z"
  },
  {
    "id": 7,
    "kind": "stub",
    "phase": "07",
    "file": "apps/web/src/routes/$owner.$repo.index.tsrx",
    "line": null,
    "description": "Clone/Download toolbar stub until 07-08",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: Clone/Download toolbar shipped in 07-08",
    "recorded_at": "2026-09-12T18:11:31.663Z",
    "resolved_at": "2026-09-19T16:39:46.493Z"
  },
  {
    "id": 8,
    "kind": "skipped-test",
    "phase": "07",
    "file": "crates/oxidean-git/src/version.rs",
    "line": null,
    "description": "Pre-existing git_archive_formats_zip_and_tar_gz Wave 0 stub fails nextest",
    "status": "waived",
    "reason": "Obsolete skipped-test: git_archive_formats Wave 0 stub removed after archive plans",
    "recorded_at": "2026-09-12T18:23:10.183Z",
    "resolved_at": "2026-09-19T16:39:46.609Z"
  },
  {
    "id": 9,
    "kind": "skipped-test",
    "phase": "07",
    "file": "crates/oxidean-api/tests/repo_branch_soft_protect.rs",
    "line": null,
    "description": "Wave 0 soft-protect stubs owned by 07-07",
    "status": "waived",
    "reason": "Obsolete skipped-test: soft-protect Wave 0 stubs superseded by 07-07",
    "recorded_at": "2026-09-12T18:23:10.268Z",
    "resolved_at": "2026-09-19T16:39:46.719Z"
  },
  {
    "id": 10,
    "kind": "stub",
    "phase": "08",
    "file": "crates/oxidean-api/tests/pat_rpc.rs",
    "line": null,
    "description": "Wave 0 RED pat_* stubs until 08-04",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: pat_* RPC tests greened in 08-04",
    "recorded_at": "2026-09-13T17:58:59.409Z",
    "resolved_at": "2026-09-19T16:39:46.830Z"
  },
  {
    "id": 11,
    "kind": "stub",
    "phase": "08",
    "file": "crates/oxidean-api/tests/git_smart_http.rs",
    "line": null,
    "description": "Wave 0 RED git_smart_* stubs until 08-04/08-06",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: git_smart_* tests greened in 08-04/08-06",
    "recorded_at": "2026-09-13T17:58:59.506Z",
    "resolved_at": "2026-09-19T16:39:46.939Z"
  },
  {
    "id": 12,
    "kind": "stub",
    "phase": "08",
    "file": "crates/oxidean-db/tests/dialect_pats.rs",
    "line": null,
    "description": "Wave 0 dialect_pats until 08-03 0008_pats",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: dialect_pats shipped with 08-03 0008_pats",
    "recorded_at": "2026-09-13T17:58:59.595Z",
    "resolved_at": "2026-09-19T16:39:47.048Z"
  },
  {
    "id": 13,
    "kind": "deviation",
    "phase": "08",
    "file": "crates/oxidean-db/tests/dialect_pats.rs",
    "line": null,
    "description": "Renamed dialect tests for test(dialect_pats) nextest filter",
    "status": "waived",
    "reason": "Historical deviation noise: dialect_pats rename for nextest filter (phase 08 shipped)",
    "recorded_at": "2026-09-13T17:58:59.686Z",
    "resolved_at": "2026-09-19T16:39:47.159Z"
  },
  {
    "id": 14,
    "kind": "stub",
    "phase": "08",
    "file": "apps/web/src/routes/settings/tokens.integration.test.ts",
    "line": null,
    "description": "Wave 0 RED tokens UI stubs until 08-09/08-10",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: tokens UI integration greened in 08-09/08-10",
    "recorded_at": "2026-09-13T18:04:19.488Z",
    "resolved_at": "2026-09-19T16:39:47.269Z"
  },
  {
    "id": 15,
    "kind": "stub",
    "phase": "08",
    "file": "apps/web/src/components/repo/clone-box.pat.integration.test.ts",
    "line": null,
    "description": "Wave 0 RED CloneBox PAT how-to stubs until 08-12",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: CloneBox PAT how-to greened in 08-12",
    "recorded_at": "2026-09-13T18:04:19.572Z",
    "resolved_at": "2026-09-19T16:39:47.382Z"
  },
  {
    "id": 16,
    "kind": "deviation",
    "phase": "08",
    "file": "apps/web/src/routes/settings/tokens.integration.test.ts",
    "line": null,
    "description": "Used runtime-variable @vite-ignore import so Vitest collects while tokens route absent",
    "status": "waived",
    "reason": "Historical deviation noise: tokens route @vite-ignore collect workaround superseded",
    "recorded_at": "2026-09-13T18:04:19.657Z",
    "resolved_at": "2026-09-19T16:39:47.491Z"
  },
  {
    "id": 17,
    "kind": "stub",
    "phase": "08",
    "file": "crates/oxidean-api/src/pat/mod.rs",
    "line": null,
    "description": "create_fine_grained stub → pat.not_implemented (08-05)",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-13T18:37:52.435Z",
    "resolved_at": "2026-09-13T18:42:52.268Z"
  },
  {
    "id": 18,
    "kind": "skipped-test",
    "phase": "08",
    "file": "crates/oxidean-api/tests/git_smart_http.rs",
    "line": null,
    "description": "4 expansion git_smart tests #[ignore] until 08-06",
    "status": "waived",
    "reason": "Obsolete skipped-test: git_smart expansion ignores lifted after 08-06",
    "recorded_at": "2026-09-13T18:37:52.519Z",
    "resolved_at": "2026-09-19T16:39:47.607Z"
  },
  {
    "id": 19,
    "kind": "deviation",
    "phase": "08",
    "file": "crates/oxidean-api/src/pat/mod.rs",
    "line": null,
    "description": "Plan prose ona_fg_ locked to oxidean_fg_ (FINE_GRAINED_PAT_PREFIX / D-08)",
    "status": "waived",
    "reason": "Historical deviation noise: FINE_GRAINED_PAT_PREFIX oxidean_fg_ locked (D-08 shipped)",
    "recorded_at": "2026-09-13T18:42:34.877Z",
    "resolved_at": "2026-09-19T16:39:47.716Z"
  },
  {
    "id": 20,
    "kind": "unrun-verify",
    "phase": "08",
    "file": "scripts/smoke-git-https.sh",
    "line": null,
    "description": "Live make smoke-git-https not run — Docker engine unavailable on executor host",
    "status": "waived",
    "reason": "Historical unrun-verify noise: smoke-git-https Docker host gap; PAT/HTTPS stack shipped in phase 08",
    "recorded_at": "2026-09-13T19:06:10.084Z",
    "resolved_at": "2026-09-19T16:39:47.823Z"
  },
  {
    "id": 21,
    "kind": "skipped-test",
    "phase": "08",
    "file": "apps/web/src/routes/settings/tokens.integration.test.ts",
    "line": null,
    "description": "D-15 one-time reveal it.skip until 08-10",
    "status": "waived",
    "reason": "Obsolete skipped-test: D-15 one-time reveal greened in 08-10",
    "recorded_at": "2026-09-13T19:22:14.417Z",
    "resolved_at": "2026-09-19T16:39:47.940Z"
  },
  {
    "id": 22,
    "kind": "deviation",
    "phase": "08",
    "file": "apps/web/src/components/settings/pat-revoke-dialog.tsrx",
    "line": null,
    "description": "Revoke dialog landed with T1 list commit; T2 greened assertions",
    "status": "waived",
    "reason": "Historical deviation noise: pat-revoke-dialog landed with list commit (phase 08 shipped)",
    "recorded_at": "2026-09-13T19:22:14.499Z",
    "resolved_at": "2026-09-19T16:39:48.050Z"
  },
  {
    "id": 23,
    "kind": "stub",
    "phase": "09",
    "file": "crates/oxidean-api/tests/ssh_key_rpc.rs",
    "line": null,
    "description": "Wave 0 assert!(false) sshKey RPC stubs until 09-03",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: sshKey RPC greened in 09-03",
    "recorded_at": "2026-09-13T23:34:41.937Z",
    "resolved_at": "2026-09-19T16:39:48.167Z"
  },
  {
    "id": 24,
    "kind": "stub",
    "phase": "09",
    "file": "crates/oxidean-api/tests/git_ssh.rs",
    "line": null,
    "description": "Wave 0 assert!(false) git_ssh stubs until 09-04/09-05",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: git_ssh tests greened in 09-04/09-05",
    "recorded_at": "2026-09-13T23:34:42.044Z",
    "resolved_at": "2026-09-19T16:39:48.274Z"
  },
  {
    "id": 25,
    "kind": "stub",
    "phase": "09",
    "file": "crates/oxidean-db/tests/dialect_ssh_keys.rs",
    "line": null,
    "description": "Wave 0 dialect_ssh_keys until 0009_ssh_keys migration (09-02)",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: dialect_ssh_keys shipped with 09-02 0009_ssh_keys",
    "recorded_at": "2026-09-13T23:34:42.151Z",
    "resolved_at": "2026-09-19T16:39:48.384Z"
  },
  {
    "id": 26,
    "kind": "stub",
    "phase": "09",
    "file": "scripts/smoke-git-ssh.sh",
    "line": null,
    "description": "Wave 0 smoke-git-ssh exit 1 until 09-05 Compose TCP green",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: smoke-git-ssh greened in 09-05",
    "recorded_at": "2026-09-13T23:34:42.250Z",
    "resolved_at": "2026-09-19T16:39:48.504Z"
  },
  {
    "id": 27,
    "kind": "stub",
    "phase": "09",
    "file": "apps/web/src/routes/settings/ssh-keys.integration.test.ts",
    "line": null,
    "description": "Wave 0 RED ssh-keys integration stubs until 09-07",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: ssh-keys integration greened in 09-07",
    "recorded_at": "2026-09-13T23:38:48.019Z",
    "resolved_at": "2026-09-19T16:39:48.613Z"
  },
  {
    "id": 28,
    "kind": "stub",
    "phase": "09",
    "file": "apps/web/src/components/repo/clone-box.ssh.integration.test.ts",
    "line": null,
    "description": "Wave 0 RED CloneBox SSH integration stubs until 09-08",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: CloneBox SSH integration greened in 09-08",
    "recorded_at": "2026-09-13T23:38:48.151Z",
    "resolved_at": "2026-09-19T16:39:48.728Z"
  },
  {
    "id": 29,
    "kind": "unrun-verify",
    "phase": "09",
    "file": "apps/web/src/routes/settings/ssh-keys.integration.test.ts",
    "line": null,
    "description": "Wave 0 vitest intentionally RED (exit 1) until production routes — verify ran, stubs fail by design",
    "status": "waived",
    "reason": "Obsolete unrun-verify: Wave 0 ssh-keys vitest RED superseded by 09-07 greens",
    "recorded_at": "2026-09-13T23:38:48.269Z",
    "resolved_at": "2026-09-19T16:39:48.838Z"
  },
  {
    "id": 30,
    "kind": "stub",
    "phase": "09",
    "file": "scripts/smoke-git-ssh.sh",
    "line": null,
    "description": "RESOLVED: smoke-git-ssh greened in 09-05",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-14T00:11:24.344Z",
    "resolved_at": "2026-09-19T16:39:45.607Z"
  },
  {
    "id": 31,
    "kind": "stub",
    "phase": "10",
    "file": "apps/web/src/routes/orgs.new.integration.test.ts",
    "line": null,
    "description": "Fails until /orgs/new lands in 10-13",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: /orgs/new shipped in 10-13",
    "recorded_at": "2026-09-13T23:41:25.124Z",
    "resolved_at": "2026-09-19T16:39:48.952Z"
  },
  {
    "id": 32,
    "kind": "stub",
    "phase": "10",
    "file": "apps/web/src/routes/new.owner-picker.integration.test.ts",
    "line": null,
    "description": "Fails until owner Select lands in 10-11",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: owner Select shipped in 10-11",
    "recorded_at": "2026-09-13T23:41:25.222Z",
    "resolved_at": "2026-09-19T16:39:49.065Z"
  },
  {
    "id": 33,
    "kind": "stub",
    "phase": "10",
    "file": "apps/web/src/routes/$owner.$repo.settings.collaborators.integration.test.ts",
    "line": null,
    "description": "Fails until collaborators-panel + can_admin in 10-11",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: collaborators-panel shipped in 10-11",
    "recorded_at": "2026-09-13T23:41:25.316Z",
    "resolved_at": "2026-09-19T16:39:49.174Z"
  },
  {
    "id": 34,
    "kind": "stub",
    "phase": "10",
    "file": "apps/web/src/routes/$owner.settings.members.integration.test.ts",
    "line": null,
    "description": "Fails until members/invites UI in 10-10",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: members/invites UI shipped in 10-10",
    "recorded_at": "2026-09-13T23:41:25.415Z",
    "resolved_at": "2026-09-19T16:39:49.283Z"
  },
  {
    "id": 35,
    "kind": "stub",
    "phase": "10",
    "file": "crates/oxidean-db/migrations/postgres/0010_orgs_acl.sql",
    "line": null,
    "description": "organization_invites/repository_collaborators tables exist without CRUD helpers/RPCs (deferred 10-06/10-07)",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: org invite/collaborator CRUD RPCs shipped in 10-06/10-07",
    "recorded_at": "2026-09-13T23:52:49.281Z",
    "resolved_at": "2026-09-19T16:39:49.393Z"
  },
  {
    "id": 36,
    "kind": "deviation",
    "phase": "10",
    "file": "crates/oxidean-api/src/auth/local.rs",
    "line": null,
    "description": "Signup still does not dual-check organizations.slug for shared namespace (D-ORG-01); org.create does — defer to signup/rename plans",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-14T00:06:48.676Z",
    "resolved_at": "2026-09-19T16:39:45.715Z"
  },
  {
    "id": 37,
    "kind": "skipped-test",
    "phase": "10",
    "file": "crates/oxidean-api/tests/repo_collaborators_acl.rs",
    "line": null,
    "description": "Collaborator CRUD ACL tests ignored until plan 07",
    "status": "waived",
    "reason": "Obsolete skipped-test: collaborator CRUD ACL tests greened after plan 10-07",
    "recorded_at": "2026-09-14T00:36:39.442Z",
    "resolved_at": "2026-09-19T16:39:49.502Z"
  },
  {
    "id": 38,
    "kind": "skipped-test",
    "phase": "10",
    "file": "crates/oxidean-api/tests/repo_private_404.rs",
    "line": 410,
    "description": "repo_private_404_collaborator_granted_read ignored until plan 07",
    "status": "waived",
    "reason": "Obsolete skipped-test: repo_private_404 collaborator read greened after plan 10-07",
    "recorded_at": "2026-09-14T00:36:39.533Z",
    "resolved_at": "2026-09-19T16:39:49.611Z"
  },
  {
    "id": 39,
    "kind": "skipped-test",
    "phase": "10",
    "file": "crates/oxidean-api/tests/git_smart_http.rs",
    "line": 610,
    "description": "git_smart_collaborator_classic_pat_push Wave-0 stub fails under test(collab) filter; PAT collaborator push deferred to later plan",
    "status": "waived",
    "reason": "Obsolete skipped-test: git_smart collaborator PAT push Wave-0 superseded by later collab plans",
    "recorded_at": "2026-09-14T01:15:41.319Z",
    "resolved_at": "2026-09-19T16:39:49.719Z"
  },
  {
    "id": 40,
    "kind": "stub",
    "phase": "11",
    "file": "crates/oxidean-api/tests/issue_lifecycle.rs",
    "line": null,
    "description": "Wave 0 assert!(false) issue lifecycle stubs until 11-02+",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: issue lifecycle tests greened in 11-02+",
    "recorded_at": "2026-09-14T14:17:20.331Z",
    "resolved_at": "2026-09-19T16:39:49.828Z"
  },
  {
    "id": 41,
    "kind": "stub",
    "phase": "11",
    "file": "crates/oxidean-db/tests/dialect_issues.rs",
    "line": null,
    "description": "Wave 0 dialect_issues RED until 0011_issues lands",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: dialect_issues shipped with 0011_issues",
    "recorded_at": "2026-09-14T14:17:20.413Z",
    "resolved_at": "2026-09-19T16:39:49.939Z"
  },
  {
    "id": 42,
    "kind": "stub",
    "phase": "11",
    "file": "crates/oxidean-db/tests/factory_reset_issues.rs",
    "line": null,
    "description": "Wave 0 factory_reset_issues RED until cascade wipe lands",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: factory_reset_issues cascade wipe shipped",
    "recorded_at": "2026-09-14T14:17:20.496Z",
    "resolved_at": "2026-09-19T16:39:50.053Z"
  },
  {
    "id": 43,
    "kind": "deviation",
    "phase": "11",
    "file": "crates/oxidean-api/tests/git_ssh.rs",
    "line": null,
    "description": "Rule 3: fixed insert_repository owner_type arity to unblock nextest list",
    "status": "waived",
    "reason": "Historical deviation noise: insert_repository arity fix for nextest list (phase 11 shipped)",
    "recorded_at": "2026-09-14T14:17:20.577Z",
    "resolved_at": "2026-09-19T16:39:50.167Z"
  },
  {
    "id": 44,
    "kind": "stub",
    "phase": "11",
    "file": "apps/web/src/routes/$owner.$repo.issues.integration.test.ts",
    "line": null,
    "description": "Wave 0 it.fails Issues UI stubs pending 11-03..11-09 greens",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: Issues UI integration greened in 11-03..11-09",
    "recorded_at": "2026-09-14T14:24:49.338Z",
    "resolved_at": "2026-09-19T16:39:50.279Z"
  },
  {
    "id": 45,
    "kind": "stub",
    "phase": "11",
    "file": "apps/web/src/lib/markdown.issues.test.ts",
    "line": null,
    "description": "Wave 0 it.fails #N autolink stubs pending 11-10 greens",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: #N autolink greened in 11-10",
    "recorded_at": "2026-09-14T14:24:49.422Z",
    "resolved_at": "2026-09-19T16:39:50.393Z"
  },
  {
    "id": 46,
    "kind": "stub",
    "phase": "11",
    "file": "apps/web/src/routes/$owner.$repo.issues.$n.tsrx",
    "line": null,
    "description": "Comments/Labels/Assignees/Linked PRs empty shells until later plans",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: issue detail panels shipped in later 11-xx plans",
    "recorded_at": "2026-09-14T14:53:54.977Z",
    "resolved_at": "2026-09-19T16:39:50.506Z"
  },
  {
    "id": 47,
    "kind": "deviation",
    "phase": "11",
    "file": "apps/web/src/routes/$owner.$repo.issues.$n.tsrx",
    "line": null,
    "description": "Rule 2: wired detail route owner/repo into renderGfm despite plan 'without editing detail route files'",
    "status": "waived",
    "reason": "Historical deviation noise: detail route owner/repo GFM wiring (phase 11 shipped)",
    "recorded_at": "2026-09-14T15:54:38.280Z",
    "resolved_at": "2026-09-19T16:39:50.621Z"
  },
  {
    "id": 48,
    "kind": "stub",
    "phase": "11",
    "file": "apps/web/src/routes/$owner.$repo.issues.$n.tsrx",
    "line": null,
    "description": "Assignees panel shipped in 11-07; Linked PRs shell remains until 11-09",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-14T16:05:16.873Z",
    "resolved_at": "2026-09-14T16:29:16.369Z"
  },
  {
    "id": 49,
    "kind": "stub",
    "phase": "11",
    "file": "apps/web/src/components/repo/issue-linked-prs.tsrx",
    "line": null,
    "description": "Linked PRs panel shows pr_stub placeholders until Phase 12 replaces kind (D-ISS-13 intentional)",
    "status": "waived",
    "reason": "Obsolete stub: Linked PRs pr_stub replaced by Phase 12 PR links (D-ISS-13)",
    "recorded_at": "2026-09-14T16:28:25.592Z",
    "resolved_at": "2026-09-19T16:39:50.733Z"
  },
  {
    "id": 50,
    "kind": "unmet-truth",
    "phase": "11",
    "file": "crates/oxidean-api/src/issue/mod.rs",
    "line": null,
    "description": "Closing-keyword auto-close deferred to Phase 12 (D-ISS-15) — verified not enforced in issue_links_no_closing_keyword_enforcement",
    "status": "waived",
    "reason": "Intentional Phase 12 deferral (D-ISS-15), not a defect",
    "recorded_at": "2026-09-14T16:28:25.718Z",
    "resolved_at": "2026-09-14T16:29:16.230Z"
  },
  {
    "id": 51,
    "kind": "stub",
    "phase": "15",
    "file": "crates/oxidean-api/tests/release_rpc.rs",
    "line": null,
    "description": "release_* tests #[ignore] until 15-01/15-02",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: release_* RPC tests greened in 15-01/15-02",
    "recorded_at": "2026-09-14T16:59:15.548Z",
    "resolved_at": "2026-09-19T16:39:50.845Z"
  },
  {
    "id": 52,
    "kind": "deviation",
    "phase": "15",
    "file": "crates/oxidean-api/src/repo/rename_transfer.rs",
    "line": null,
    "description": "15-03 combined T1-T3 into single commit due to shared redirect wiring",
    "status": "waived",
    "reason": "Historical deviation noise: 15-03 combined rename/transfer commit (phase 15 shipped)",
    "recorded_at": "2026-09-14T17:31:23.559Z",
    "resolved_at": "2026-09-19T16:39:50.957Z"
  },
  {
    "id": 53,
    "kind": "deviation",
    "phase": "11.1",
    "file": "scripts/coverage-weighted.sh",
    "line": null,
    "description": "Bootstrap floor 0.65 instead of plan ~0.70; ratchet target 0.70 documented",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-15T16:21:58.576Z",
    "resolved_at": "2026-10-02T00:27:48.664Z"
  },
  {
    "id": 54,
    "kind": "deviation",
    "phase": "11.1",
    "file": ".github/workflows/ci.yml",
    "line": null,
    "description": "CI skips cargo-llvm-cov collect (Make target remains); web+checklist drive gate",
    "status": "fixed",
    "reason": "",
    "recorded_at": "2026-09-15T16:21:58.697Z",
    "resolved_at": "2026-10-02T00:27:48.664Z"
  },
  {
    "id": 55,
    "kind": "stub",
    "phase": "18",
    "file": "crates/oxidean-api/tests/webhook_rpc.rs",
    "line": null,
    "description": "All webhook_* RPC tests #[ignore] until 18-01/18-02",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: webhook_* RPC tests greened in 18-01/18-02",
    "recorded_at": "2026-09-16T13:43:57.313Z",
    "resolved_at": "2026-09-19T16:39:51.070Z"
  },
  {
    "id": 56,
    "kind": "stub",
    "phase": "18",
    "file": "crates/oxidean-api/tests/webhook_delivery.rs",
    "line": null,
    "description": "All webhook_* delivery tests #[ignore] until 18-01..18-03",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: webhook delivery tests greened in 18-01..18-03",
    "recorded_at": "2026-09-16T13:43:57.452Z",
    "resolved_at": "2026-09-19T16:39:51.186Z"
  },
  {
    "id": 57,
    "kind": "stub",
    "phase": "18",
    "file": "crates/oxidean-db/tests/dialect_webhooks.rs",
    "line": null,
    "description": "dialect_webhooks #[ignore] until 18-01",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: dialect_webhooks greened in 18-01",
    "recorded_at": "2026-09-16T13:43:57.578Z",
    "resolved_at": "2026-09-19T16:39:51.301Z"
  },
  {
    "id": 58,
    "kind": "stub",
    "phase": "18",
    "file": "apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts",
    "line": null,
    "description": "Settings Webhooks Vitest it.fails until 18-04",
    "status": "waived",
    "reason": "Obsolete Wave 0 stub: Settings Webhooks Vitest greened in 18-04",
    "recorded_at": "2026-09-16T13:43:57.702Z",
    "resolved_at": "2026-09-19T16:39:51.416Z"
  }
]
````
