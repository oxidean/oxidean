---
phase: "07"
slug: "git-repos-browse"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
block_on: high
created: "2026-09-13"
---

# Phase 07 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Client → `repo.create` / mutate RPCs | Untrusted name, visibility, refs, templates | Session cookie; repo metadata |
| Client → raw / archive HTTP | Untrusted owner/repo/ref/path/treeish | Public or ACL-gated blob/archive bytes |
| API → filesystem `repos_dir` | Bare repo paths must stay under canonical root | Owner/name → `{repos_dir}/{owner}/{name}.git` |
| API → git argv (`CliGitBackend`) | User-influenced refs/names must not become options | Argv arrays only; end-of-options `--` |
| Browser → Code / Markdown UI | Untrusted README / blob text | Sanitized HTML; soft size caps |
| Sys-admin → factory reset / GC / purge | Destructive instance and disk ops | `RESET` confirm; scope enum; path canon |
| Test harness → temp `repos_dir` | Parallel nextest isolation | Temp dirs / `OXIDEAN_REPOS_DIR` |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-07-01 | Elevation of privilege | `repo.create` / `/new` | high | mitigate | `require_verified` before create; stubs assert `auth.email_unverified` (D-11) | closed |
| T-07-02 | Information disclosure | private ACL stubs | high | mitigate | Identical `repo.not_found` for missing and non-owner private (D-25) | closed |
| T-07-03 | Tampering | GIT-09 / ARCHITECTURE docs | medium | mitigate | CLI-primary + future gix documented (D-32); REQUIREMENTS/ROADMAP amended in 07-01/11 | closed |
| T-07-04 | Denial of service | Boot without git | high | mitigate | Human door D-33; `assert_git_version((2,5,0))` fail-boot in `main` (T-07-08) | closed |
| T-07-05 | Elevation of privilege | `repo.create` | high | mitigate | `require_verified`; bootstrap lock while `needs_setup` | closed |
| T-07-05a | Tampering | repositories uniqueness | medium | mitigate | Unique among non-deleted `(owner_id, lower(name))`; soft-delete column | closed |
| T-07-05b | Elevation of privilege | `/new` UI | medium | mitigate | Verify wall + disabled CTA; server still `require_verified` | closed |
| T-07-06 | Tampering | `CliGitBackend` | high | mitigate | `tokio::process::Command` argv arrays only; `validate_repo_name` before path join | closed |
| T-07-07 | Tampering | `repos_dir` paths | high | mitigate | Absolutize / join under `OXIDEAN_REPOS_DIR`; basename guards | closed |
| T-07-08 | Denial of service | missing git | high | mitigate | Boot `assert_git_version((2,5,0))` → exit 1 (D-33) | closed |
| T-07-09 | Tampering | template asset paths | high | mitigate | Pack IDs via allowlist maps under embedded `assets/`; reject `..` | closed |
| T-07-10 | Information disclosure | duplicate create | low | accept | `repo.name_taken` expected UX for owner namespace | closed |
| T-07-11 | Information disclosure | `repo.listMine` | medium | mitigate | Return only caller-owned non-deleted repos | closed |
| T-07-12 | Elevation of privilege | instance `default_visibility` | high | mitigate | Reuse existing admin gate patterns | closed |
| T-07-13 | Information disclosure | repo / history ACL | high | mitigate | Unified `acl.rs` `not_found` on every browse/history procedure | closed |
| T-07-13b | Information disclosure | Code UI | high | mitigate | Private → Not found page identical to missing (D-25) | closed |
| T-07-14 | Tampering / XSS | `markdown.ts` | high | mitigate | `rehype-sanitize` after remark-rehype; no raw HTML passthrough | closed |
| T-07-15 | Tampering | raw route | high | mitigate | Validate ref/path; resolve under `repos_dir`; soft truncate | closed |
| T-07-16 | Denial of service | huge blobs | medium | mitigate | Soft size limit + headers (`BLOB_SOFT_MAX_BYTES` / D-20) | closed |
| T-07-17 | Tampering | ref/sha / branch names | high | mitigate | Allowlist / `validate_treeish`; reject path escapes before Command | closed |
| T-07-18 | Denial of service | unbounded diff | medium | mitigate | Soft patch byte / file caps; `truncated` flag | closed |
| T-07-19 | Elevation of privilege | branch mutate | high | mitigate | Owner-only mutate (`resolve_repo_for_owner_mutate`); UI hides for non-owners | closed |
| T-07-20 | Tampering | default branch | medium | mitigate | API rejects rename/delete of default branch (D-28) | closed |
| T-07-21 | Information disclosure | archive ACL | high | mitigate | Same `resolve_repo_for_read` before `git archive` | closed |
| T-07-22 | Denial of service | large archive | medium | mitigate | Collect under timeout; only `.zip` / `.tar.gz` | closed |
| T-07-23 | Elevation of privilege | settings RPCs | high | mitigate | Owner-only; non-owner unified `not_found` | closed |
| T-07-24 | Tampering | soft-delete confirm | medium | mitigate | Typed `confirm_name` must match server-side | closed |
| T-07-25 | Elevation of privilege | factory_reset | high | mitigate | Admin gate + typed `RESET`; scope validated server-side | closed |
| T-07-26 | Tampering | orphan purge paths | high | mitigate | `delete_under_repos_dir` canonicalize; refuse escapes | closed |
| T-07-27 | Denial of service | gc storm | low | accept | Manual + modest schedule; frequency documented | closed |
| T-07-GC19-01 | Tampering | `branch_create` / `validate_treeish` | high | mitigate | Reject trimmed refs beginning with `-`; `--` before operands (CR-02) | closed |
| T-07-GC19-02 | Elevation of privilege | `repo.branchCreate` | high | mitigate | `reject_option_like_branch` before CLI (D-28) | closed |
| T-07-GC19-03 | Tampering | `branch_rename` / `branch_delete` | high | mitigate | End-of-options before from/to/name | closed |
| T-07-GC20-01 | Tampering | `CliGitBackend::archive` | critical | mitigate | End-of-options before treeish; `validate_treeish` leading-`-` reject | closed |
| T-07-GC20-02 | Tampering | archive/raw validators | critical | mitigate | Reject leading `-` at HTTP boundary before spawn | closed |
| T-07-GC20-03 | Information disclosure | raw vs archive ref rules | medium | mitigate | Allow `/` in raw `validate_ref` (WR-02); keep `..`/NUL/metachar/`-` rejects | closed |
| T-07-GC21-01 | Denial of service | `repo.create` partial failure | medium | mitigate | Soft-delete + remove partial bare dir on init/seed failure (WR-01) | closed |
| T-07-GC21-02 | Tampering | `parseRefAndPath` | low | accept | Client parse only; server validates refs | closed |
| T-07-SC | Tampering | packages / distro git | high | mitigate | No unplanned crates.io/npm; official shadcn / RESEARCH-pinned only; apt git accepted in image | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above `workflow.security_block_on` (`high`) count toward `threats_open`*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

### Evidence (ASVS L1)

| Threat ID | Evidence |
|-----------|----------|
| T-07-01 / T-07-05 / T-07-05b | `auth/gate.rs` `require_verified`; `repo/mod.rs` `create` calls it; `/new` verify wall |
| T-07-02 / T-07-13 / T-07-13b / T-07-21 | `repo/acl.rs` unified `not_found`; archive uses `resolve_repo_for_read` |
| T-07-03 | ARCHITECTURE / REQUIREMENTS GIT-09 CLI-primary wording (07-01/11 summaries) |
| T-07-04 / T-07-08 | `oxidean-git/src/version.rs` + `main.rs` exit 1 on version Err |
| T-07-05a | Soft-delete + uniqueness among non-deleted (migrations / dialect_repositories) |
| T-07-06 | `CliGitBackend` argv-only `run_git`; `validate_repo_name` before path join |
| T-07-07 / T-07-26 | `app.rs` absolutize `repos_dir`; `jobs/reconcile.rs` `delete_under_repos_dir` |
| T-07-09 | `repo/templates.rs` allowlist `is_safe_id`; embedded assets |
| T-07-10 | Accepted Risks Log — `repo.name_taken` |
| T-07-11 | `repo::list_mine` filters by session owner |
| T-07-12 | Admin settings gate reused for instance default visibility |
| T-07-14 | `apps/web/src/lib/markdown.ts` `rehypeSanitize` |
| T-07-15 / T-07-GC20-02/03 | `routes/repo_raw.rs` `validate_ref` / `validate_blob_path` / slashy refs |
| T-07-16 | `BLOB_SOFT_MAX_BYTES` + `x-oxidean-blob-*` headers |
| T-07-17 / T-07-GC19-* | `cli.rs` `validate_treeish` + `--` on branch ops; `reject_option_like_branch` |
| T-07-18 | `backend.rs` soft patch caps; `truncated` on show/diff |
| T-07-19 / T-07-20 / T-07-23 / T-07-24 | `resolve_repo_for_owner_mutate`; default-branch protect; `confirm_name` |
| T-07-22 | Archive formats zip/tar.gz only; collect with timeout |
| T-07-25 | `auth/admin.rs` `require_admin` + `confirmation == "RESET"` |
| T-07-27 | Accepted Risks Log |
| T-07-GC20-01 | `cli.rs` archive argv ends with `--` + validated treeish |
| T-07-GC21-01 | `compensate_failed_create` on init_bare/seed_commit Err |
| T-07-GC21-02 | Accepted Risks Log |
| T-07-SC | Plan summaries: no unplanned packages; RESEARCH-pinned shiki/unified/spdx; distro git |

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-07-10 | T-07-10 | Duplicate name error discloses only that the caller's own namespace already has that name — expected forge UX. | phase-07 secure audit | 2026-09-13 |
| AR-07-27 | T-07-27 | GC is admin-triggered / modest schedule; storm risk accepted vs automatic aggressive GC. | phase-07 secure audit | 2026-09-13 |
| AR-07-GC21-02 | T-07-GC21-02 | Client `parseRefAndPath` is UX-only; wrong splits fail browse; server validates refs/paths. | phase-07 secure audit | 2026-09-13 |

*Accepted risks do not resurface in future audit runs.*

---

## Unregistered Flags

None — SUMMARY.md `## Threat Flags` entries map to plan threat IDs (T-07-01…T-07-27 / GC19–21 / T-07-SC).

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open (blocking) | Open (non-blocking) | Run By |
|------------|---------------|--------|-----------------|---------------------|--------|
| 2026-09-13 | 39 | 39 | 0 | 0 | secure-phase orchestrator (ASVS L1 short-circuit; state B) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-09-13
