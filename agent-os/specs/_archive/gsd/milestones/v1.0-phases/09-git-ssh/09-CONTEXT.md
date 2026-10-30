# Phase 9: Git SSH - Context

**Gathered:** 2026-09-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Users register SSH public keys and clone/fetch/push over SSH like a normal forge remote. Delivers GIT-03 and GIT-04.

**Requirements:** GIT-03, GIT-04

**Success criteria (from ROADMAP):**
1. User can add, list, and revoke SSH public keys on their account
2. User can clone, fetch, and push over SSH with a registered public key

**Out of scope (later phases / v2):**
- Org/collaborator ACL beyond current owner-only private model (Phase 10 extends ACL; SSH must call the same ACL module)
- Git LFS over SSH (Phase 14)
- Interactive shell, SFTP, port-forward, scp
- SSH certificate authority / org-managed host CAs
- PAT scopes on SSH (keys = account identity, not fine-grained tokens)

**UI hint:** yes — `/settings/ssh-keys`, CloneBox live SSH URL + compact how-to (replaces Phase 7/8 placeholder).

</domain>

<decisions>
## Implementation Decisions

### A — Server topology
- **D-SSH-01:** Serve git-over-SSH via a **Rust SSH service/module** (e.g. russh) that authenticates registered public keys against Oxidean DB/ACL and spawns system `git-upload-pack` / `git-receive-pack` on the shared `OXIDEAN_REPOS_DIR` volume. **Not** OpenSSH/`git-shell`. No interactive shell/SFTP/port-forward — **git pack commands only** — **Reversibility:** costly — Compose/service boundary + deploy story

### B — Clone URL + port
- **D-SSH-02:** Public clone URL is always scp-style **`git@{OXIDEAN_SSH_HOST}:{owner}/{repo}.git`** (GitHub/Gitea-shaped). Compose/dev default listen port **2222**; when advertised port ≠ 22, document `~/.ssh/config` Port or Host alias (do **not** make `ssh://` the primary CloneBox string). Production/cloud prefer port **22** when the platform allows; `OXIDEAN_SSH_HOST` + `OXIDEAN_SSH_PORT` (listen/advertise) configurable. Host fallback: hostname of public origin — **Reversibility:** one-way — published remote URL contract

### C — Principal
- **D-SSH-03:** Force SSH login user **`git` only** (GitHub/Gitea-style). Account identity is derived solely from the registered public key fingerprint; the SSH username is not an Oxidean account name — **Reversibility:** costly — client docs + reserved `git` principal

### D — ACL parity with Smart HTTP
- **D-SSH-04:** Key maps to the **account** (no PAT scopes). Public fetch OK when authenticated; private = **owner-only** until Phase 10 collaborators; **push requires verified email** (same as Smart HTTP). Deny private non-owner with a clear **git error** (not HTTP 401/404) — **Reversibility:** costly — must stay aligned with `repo/acl` + Phase 10

### E — Key registration (defaults — discuss skipped)
- **D-SSH-05:** Required **title/note**; store full public key + **fingerprint** (unique); accept **ed25519** + **rsa-sha2**; max ~**25** keys per user; add/list/revoke over session RPC; **`require_verified`** to add; **confirm** on revoke (no one-time secret reveal — public keys)

### F — Settings + CloneBox (defaults — discuss skipped)
- **D-SSH-06:** Manage keys at **`/settings/ssh-keys`** as SettingsNav sibling to tokens; replace CloneBox SSH placeholder with copyable SSH URL + compact “add a key” CTA (PatHowTo-style)

### G — Ops / smoke (defaults — discuss skipped)
- **D-SSH-07:** Do **not** route SSH through Traefik HTTP; publish **TCP** on the SSH service. Rate-limit failed pubkey auth (per IP / per key fingerprint). Track **last-used** on keys. Add **`make smoke-git-ssh`** beside HTTPS smoke; Compose-first for local SSH (API-only `make dev` may omit SSH listener unless explicitly enabled)

### Claude's Discretion
- Exact Rust SSH crate/version (russh vs alternatives) and process layout (in-api module vs sibling binary/service)
- Exact host-key storage/rotation and failure log fields
- Exact rate-limit N/window (within D-SSH-07)
- Whether advertised port is a separate env from listen port or one `OXIDEAN_SSH_PORT` with docs

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 9 goal, GIT-03/04
- `.planning/REQUIREMENTS.md` — GIT-03, GIT-04
- `.planning/PROJECT.md` — remotes must work as `git@…:user/repo.git`
- `.planning/phases/07-git-repos-browse/07-CONTEXT.md` — D-22 HTTPS now / SSH placeholder
- `.planning/phases/08-git-https-pats/08-CONTEXT.md` — Smart HTTP ACL/auth gates to mirror (D-20/D-21/D-24)

### Code mirrors
- `crates/oxidean-api/src/routes/git_smart_http.rs` — auth/ACL/rate-limit matrix
- `crates/oxidean-api/src/repo/acl.rs` — owner-only read stub (Phase 10 extends)
- `crates/oxidean-api/src/git/http_backend.rs` — pack spawn pattern
- `apps/web/src/components/repo/clone-box.tsrx` — SSH placeholder to replace
- `apps/web/src/routes/settings/tokens*.tsrx` — settings CRUD analog for keys
- `docker-compose.yml` — Traefik HTTP-only today; SSH needs TCP publish
- `scripts/smoke-git-https.sh` — pattern for `smoke-git-ssh`

</canonical_refs>

<code_context>
## Reusable assets
- Smart HTTP CGI spawn + bare path `{repos_dir}/{owner}/{name}.git`
- PAT settings list/create/revoke + confirm dialog patterns
- `require_verified` gate
- Reserved username `git` (already reserved for PAT aliases)

## Gaps
- No SSH implementation, tables, RPC, Compose port, or host keys yet
</code_context>

<deferred>
## Deferred Ideas
- SSH CA / certificate auth
- Org-deploy keys / read-only deploy keys as a named product (may overlap Phase 10 collaborators — keep distinct if shipped later)
</deferred>
