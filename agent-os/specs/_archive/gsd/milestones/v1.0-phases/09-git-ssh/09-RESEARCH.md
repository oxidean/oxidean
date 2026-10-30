# Phase 9: Git SSH - Research

**Researched:** 2026-09-14
**Domain:** Git-over-SSH (russh) + SSH public-key CRUD + Compose TCP publish
**Confidence:** HIGH (codebase mirrors + locked CONTEXT); MEDIUM (russh 0.63 channel I/O details from docs.rs / community)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-SSH-01:** Serve git-over-SSH via a **Rust SSH service/module** (e.g. russh) that authenticates registered public keys against Oxidean DB/ACL and spawns system `git-upload-pack` / `git-receive-pack` on the shared `OXIDEAN_REPOS_DIR` volume. **Not** OpenSSH/`git-shell`. No interactive shell/SFTP/port-forward — **git pack commands only**
- **D-SSH-02:** Public clone URL is always scp-style **`git@{OXIDEAN_SSH_HOST}:{owner}/{repo}.git`**. Compose/dev default listen port **2222**; when advertised port ≠ 22, document `~/.ssh/config` Port or Host alias (do **not** make `ssh://` the primary CloneBox string). Production/cloud prefer port **22** when the platform allows; `OXIDEAN_SSH_HOST` + `OXIDEAN_SSH_PORT` (listen/advertise) configurable. Host fallback: hostname of public origin
- **D-SSH-03:** Force SSH login user **`git` only**. Account identity is derived solely from the registered public key fingerprint; the SSH username is not an Oxidean account name
- **D-SSH-04:** Key maps to the **account** (no PAT scopes). Public fetch OK when authenticated; private = **owner-only** until Phase 10 collaborators; **push requires verified email** (same as Smart HTTP). Deny private non-owner with a clear **git error** (not HTTP 401/404)
- **D-SSH-05:** Required **title/note**; store full public key + **fingerprint** (unique); accept **ed25519** + **rsa-sha2**; max ~**25** keys per user; add/list/revoke over session RPC; **`require_verified`** to add; **confirm** on revoke (no one-time secret reveal — public keys)
- **D-SSH-06:** Manage keys at **`/settings/ssh-keys`** as SettingsNav sibling to tokens; replace CloneBox SSH placeholder with copyable SSH URL + compact “add a key” CTA (PatHowTo-style)
- **D-SSH-07:** Do **not** route SSH through Traefik HTTP; publish **TCP** on the SSH service. Rate-limit failed pubkey auth (per IP / per key fingerprint). Track **last-used** on keys. Add **`make smoke-git-ssh`** beside HTTPS smoke; Compose-first for local SSH (API-only `make dev` may omit SSH listener unless explicitly enabled)

### Claude's Discretion
- Exact Rust SSH crate/version (russh vs alternatives) and process layout (in-api module vs sibling binary/service)
- Exact host-key storage/rotation and failure log fields
- Exact rate-limit N/window (within D-SSH-07)
- Whether advertised port is a separate env from listen port or one `OXIDEAN_SSH_PORT` with docs

### Deferred Ideas (OUT OF SCOPE)
- SSH CA / certificate auth
- Org-deploy keys / read-only deploy keys as a named product (may overlap Phase 10 collaborators — keep distinct if shipped later)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| GIT-03 | User can clone, fetch, and push over SSH with a registered public key | russh server + pack spawn + ACL parity with Smart HTTP; Compose TCP 2222; smoke-git-ssh |
| GIT-04 | User can add, list, and revoke SSH public keys on their account | `ssh_public_keys` migration + session RPC + `/settings/ssh-keys` Octane UI mirroring PAT settings |
</phase_requirements>

## Summary

Phase 9 adds forge-shaped Git SSH on top of the Phase 7 bare-repo layout and Phase 8 ACL/auth gates. The locked approach is a **Rust-native SSH listener (russh)** inside the API process (recommended), not OpenSSH/`git-shell`. Clients connect as user `git`, authenticate with a registered public key, and may only `exec` `git-upload-pack` / `git-receive-pack` against `{OXIDEAN_REPOS_DIR}/{owner}/{name}.git`. Identity is **fingerprint → user**, never the SSH username. ACL must call the same owner-only helpers Smart HTTP uses today (`can_read_as_owner` / `is_private_visibility`), with **git stderr errors** instead of HTTP 401/404, and **verified email required on push**. Key CRUD mirrors PAT settings (session RPC + `require_verified` + confirm revoke) without a one-time secret reveal.

Ops differ from HTTPS: SSH is **raw TCP** (Compose publish `2222:2222`), never Traefik HTTP. CloneBox shows scp-style `git@host:owner/repo.git` and documents non-22 ports via `~/.ssh/config`. Smoke follows `scripts/smoke-git-https.sh` patterns.

**Primary recommendation:** Ship `russh 0.63` as an in-process Tokio task in `oxidean-api`, table `ssh_public_keys` (migration `0009`), RPC under `ssh.*`, UI at `/settings/ssh-keys`, TCP publish in Compose, and `make smoke-git-ssh`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| SSH key add/list/revoke | API / Backend | Browser / Client | Session RPC + DB; UI is CRUD shell |
| Public-key parse + fingerprint uniqueness | API / Backend | Database / Storage | Validate OpenSSH lines; UNIQUE fingerprint |
| SSH transport + pubkey auth | API / Backend | — | russh listener; not Traefik/web |
| Pack protocol (upload/receive) | API / Backend | Database / Storage | Spawn system git-*pack on bare path |
| Repo ACL (private/owner/push/verify) | API / Backend | — | Shared `repo/acl` decisions; Phase 10 extends |
| Clone URL display | Browser / Client | Frontend Server (SSR) | CloneBox + `OXIDEAN_SSH_HOST`/`PORT` |
| TCP publish / host keys | CDN/Static ops → Compose | API / Backend | Host port map + persisted host key volume |
| Rate-limit failed auth | API / Backend | — | In-process limiter (mirror PAT) |

## Project Constraints (from .cursor/rules/)

- One product (cloud + self-host); Bun workspaces + Cargo crates — no parallel app structure. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]
- Web UI is **Octane** (`.tsrx`); load `.agents/skills/octane/SKILL.md` before UI edits; no JSX/`return (` mixed with Rivet. [VERIFIED: `.cursor/rules/octane-ui.mdc`]
- RPC: change Rust → `make rpc-gen`; never hand-edit `packages/api-client` as source of truth. [VERIFIED: `.cursor/rules/rpc-codegen.mdc`]
- Dialect SQL only in `crates/oxidean-db`. [VERIFIED: `.cursor/rules/rust-crates.mdc`]
- Prefer extending existing patterns (auth gates, Make targets, Query session helpers). [VERIFIED: `.cursor/rules/oxidean-core.mdc`]
- Prefer `Result` + structured errors; no `unwrap`/`expect` outside tests. [VERIFIED: `.cursor/rules/rust-crates.mdc`]
- Preserve `require_verified` / destructive confirmations. [VERIFIED: `.cursor/rules/rust-crates.mdc`]

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `russh` | **0.63.3** (semver `0.63`) | Async SSH 2.0 server (Tokio) | Locked direction (D-SSH-01); crates.io current; `Handler` + `Channel::make_reader`/`make_writer` for pack bridging [VERIFIED: crates.io `cargo info russh` → 0.63.3; docs.rs Handler/Channel] |
| System `git` | ≥ 2.5 (already gated) | `git-upload-pack` / `git-receive-pack` | Matches GIT-09 / CliGitBackend; same binary image as Smart HTTP [VERIFIED: `main.rs` git gate; `http_backend.rs` CGI pattern] |
| `ssh-key` | **pin `=0.7.0-rc.11`** (russh dependency) | Parse OpenSSH public keys + SHA256 fingerprints for registration | Same crate russh 0.63.3 depends on (`ssh-key =0.7.0-rc.11`) — avoid dual PublicKey types [VERIFIED: crates.io dependencies API for russh 0.63.3] |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| Existing `tokio` (workspace) | workspace | Spawn SSH listener + pack processes | Already in `oxidean-api` |
| Existing `FailedAuthLimiter` pattern | in-tree | Failed pubkey rate limits | Extend or clone for IP + fingerprint buckets |
| Octane + existing settings UI | in-tree | `/settings/ssh-keys`, SettingsNav, CloneBox | Mirror tokens routes |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| russh in-process | Sibling `oxidean-ssh` binary | Cleaner crash isolation; worse for shared `AppState`/DB in v1 — defer |
| russh | OpenSSH + `AuthorizedKeysCommand` | Forbidden by D-SSH-01 |
| russh | `thrussh` | Predecessor; **SUS** low downloads — do not use [VERIFIED: package-legitimacy] |

**Installation:**

```toml
# crates/oxidean-api/Cargo.toml
russh = { version = "0.63", features = ["aws-lc-rs"] }  # default crypto; RSA via default features
ssh-key = { version = "=0.7.0-rc.11", features = ["std"] }  # match russh pin exactly
```

**Version verification:** `cargo info russh` → **0.63.3** (2026-09-14). `russh` pins `ssh-key =0.7.0-rc.11`. Prefer matching that pin over max-stable `0.6.7` to share types with auth callbacks. [VERIFIED: crates.io]

**Discretion — process layout:** **In-process module** (`crates/oxidean-api/src/ssh/`) started from `main.rs` via `tokio::spawn` when `OXIDEAN_SSH_ENABLED` is true/1 (Compose sets it). Shares `AppState` (db, repos_dir, limiter). Sibling binary deferred.

**Discretion — env ports:**

| Env | Role | Compose default | Prod preference |
|-----|------|-----------------|-----------------|
| `OXIDEAN_SSH_ENABLED` | Start listener | `true` | operator |
| `OXIDEAN_SSH_HOST` | Advertised hostname for CloneBox | hostname of `OXIDEAN_PUBLIC_ORIGIN` (fallback `localhost`) | public DNS |
| `OXIDEAN_SSH_PORT` | **Both** listen bind port **and** advertised port | `2222` | `22` when platform allows |
| `OXIDEAN_SSH_HOST_KEY_DIR` | Persist host keys | `/var/ssh` (volume) | same |

Document: if host publishes a different external port than container listen, operators must set `Port` in `~/.ssh/config` (CloneBox still shows scp-style without `ssh://`). Single `OXIDEAN_SSH_PORT` keeps operator surface small (discretion choice).

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `russh` | crates | since 2022-03 | ~216k/wk | github.com/warp-tech/russh | OK | Approved |
| `ssh-key` | crates | since 2021-11 | ~361k/wk | github.com/RustCrypto/SSH | OK | Approved (pin russh’s `=0.7.0-rc.11`) |
| `thrussh` | crates | since 2016 | ~546/wk | nest.pijul.com/pijul/ssh | SUS | REMOVED — do not recommend |

**Packages removed due to [SLOP] verdict:** none  
**Packages flagged as suspicious [SUS]:** `thrussh` (removed from stack)

## Architecture Patterns

### System Architecture Diagram

```text
git client
   │  ssh -p PORT git@HOST  (scp: git@HOST:owner/repo.git)
   ▼
TCP :OXIDEAN_SSH_PORT  ──►  russh Server (in oxidean-api)
                               │
                               ├─ auth: user must be "git"
                               ├─ auth_publickey → fingerprint lookup → user_id
                               ├─ rate-limit fail (IP / fingerprint)
                               │
                               └─ exec_request only:
                                    git-upload-pack | git-receive-pack
                                         │
                                         ├─ parse 'owner/repo.git'
                                         ├─ bare_repo_path(repos_dir,…)
                                         ├─ ACL (public+authed / private owner /
                                         │       push + email_verified)
                                         ├─ touch last_used
                                         └─ spawn git-*pack ↔ channel I/O

Browser ──session RPC──► ssh.keys.* ──► ssh_public_keys (oxidean-db)
Browser CloneBox ── reads OXIDEAN_SSH_HOST/PORT ──► scp-style URL

Traefik HTTP ── (unchanged) Smart HTTP / web / API
SSH ── NOT via Traefik ── host port publish only
```

### Recommended Project Structure

```text
crates/oxidean-db/migrations/{sqlite,postgres,mysql}/0009_ssh_keys.sql
crates/oxidean-api/src/ssh/
  mod.rs              # enable/listen bootstrap
  server.rs           # russh Server + Handler
  auth.rs             # fingerprint → user; force user git
  pack.rs             # parse exec; spawn upload/receive-pack
  rate_limit.rs       # or reuse/extend pat::rate_limit
  host_keys.rs        # load/generate host key under HOST_KEY_DIR
crates/oxidean-api/src/ssh_keys/   # RPC CRUD (mirror pat/)
crates/oxidean-core/src/ssh_types.rs
apps/web/src/routes/settings/ssh-keys.tsrx
apps/web/src/components/repo/ssh-how-to.tsrx
scripts/smoke-git-ssh.sh
```

### Pattern 1: Force `git` + identity from key

**What:** Reject any SSH username ≠ `git` (case-sensitive forge convention). Map `PublicKey` → `SHA256:…` fingerprint → `ssh_public_keys` row → `users`.  
**When to use:** Every connection (D-SSH-03).  
**Note:** `"git"` is already reserved for accounts. [VERIFIED: `crates/oxidean-core/src/auth_types.rs:278-281` — `"git",` `"token",` `"oauth2",`]

### Pattern 2: Allowlisted pack exec + path safety

**What:** Accept only `git-upload-pack` / `git-receive-pack` (optional: reject `git-upload-archive` in v1). Parse path argument; strip single quotes; reject `/`, `..`, absolute paths; resolve via existing `bare_repo_path`.  
**When to use:** `Handler::exec_request`.  
**Channel I/O:** Keep `Channel` from `channel_open_session`; in `exec_request` use `make_reader` / `make_writer` (or `into_stream`) with `tokio::process::Command` stdio. [CITED: docs.rs/russh/0.63.3 Channel; Eugeny/russh#313]

### Pattern 3: ACL parity with Smart HTTP (git errors)

Mirror `authorize_and_cgi` decisions [VERIFIED: `git_smart_http.rs:400-434`]:

| Case | Smart HTTP | SSH (D-SSH-04) |
|------|------------|----------------|
| Unauthenticated public fetch | allowed | N/A — SSH always keyed; authenticated public fetch OK |
| Unauthenticated private | 401 | auth reject / no session |
| Authed non-owner private | 401 | clear git stderr error (not HTTP) |
| Push non-owner | 401 | git stderr deny |
| Push unverified email | `auth.email_unverified` JSON | git stderr: email verification required |
| PAT scopes | apply | **none** — full account identity |

Reuse `is_private_visibility` / `can_read_as_owner` from `repo/acl.rs` [VERIFIED: `acl.rs:17-24`]. Phase 10 will centralize further — call shared helpers now so SSH does not fork ACL logic.

### Pattern 4: Key registration (RPC + DB)

Mirror PAT table shape (timestamps as TEXT RFC3339 / dialect-friendly), but store **public key material** (not secret hash):

Suggested columns: `id`, `user_id`, `title`, `public_key` (OpenSSH one-line), `fingerprint` (**UNIQUE**), `key_type`, `last_used_at`, `created_at`. Soft-delete optional (`revoked_at`) vs hard delete on revoke — prefer **hard delete** (public keys; simpler). Cap **25** keys/user in create RPC. Algorithms: `ssh-ed25519`; RSA with **≥2048 bits** and signature alg rsa-sha2 (reject weak/`ssh-rsa` SHA-1 at auth if russh config allows). `require_verified` on add [same as PAT create].

Fingerprint storage: OpenSSH display form `SHA256:…` (no trailing `=`). [CITED: docs.rs/ssh-key Fingerprint example `SHA256:Nh0Me49Zh9fDw/VYUfq43IJmI1T+XrjiYONPND8GzaM`]

### Pattern 5: Settings + CloneBox

- Extend `SettingsNav` active union `"profile" | "tokens" | "ssh-keys"`; chrome Account menu sibling link. [VERIFIED: `settings-nav.tsrx:1-31`]
- Replace CloneBox muted placeholder [VERIFIED: `clone-box.tsrx:130-132`] with SSH URL + compact how-to CTA → `/settings/ssh-keys` (PatHowTo analog).
- Helper `sshCloneUrl(host, port, owner, repo)` → always `git@{host}:{owner}/{repo}.git`; if `port !== 22`, show one-line Port hint (do not switch primary string to `ssh://`).

### Anti-Patterns to Avoid

- **OpenSSH/`git-shell` AuthorizedKeysCommand:** Violates D-SSH-01; harder DB identity.
- **Routing SSH through Traefik HTTP:** Violates D-SSH-07; use host TCP publish.
- **Primary CloneBox `ssh://git@host:2222/...`:** Violates D-SSH-02.
- **Using SSH username as Oxidean account:** Violates D-SSH-03.
- **Applying PAT fine-grained scopes to SSH:** Violates D-SSH-04.
- **Hand-editing api-client:** Use `make rpc-gen`.
- **Dialect SQL in api crate:** Migrations + CRUD in `oxidean-db` only.
- **Accepting shell/pty/subsystem/tcpip_forward:** Attack surface; always `channel_failure`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SSH protocol / crypto | Custom SSH | `russh` | Terrapin, KEX, host keys, auth timing |
| OpenSSH key parse / fingerprint | Manual base64 SHA | `ssh-key` `PublicKey::fingerprint` | Padding/`SHA256:` format bugs |
| Pack protocol framing | Custom pkt-line server | System `git-*pack` | Same as forge industry practice |
| Path traversal to repos | String concat | Existing `bare_repo_path` | Already rejects `..` `/` `\` [VERIFIED: `git/mod.rs:16-31`] |
| Failed-auth flooding | Ad-hoc sleeps only | Sliding-window limiter (mirror PAT) | Consistent ops story |
| Settings CRUD UI kit | New design system | Existing tokens/AlertDialog patterns | Brand + Octane consistency |

**Key insight:** Authentication and ACL belong in Oxidean; pack bytes belong to system git. russh is only the transport gate.

## Common Pitfalls

### Pitfall 1: Dropping `Channel` before exec
**What goes wrong:** `exec_request` cannot bridge stdio.  
**Why:** Channel must be retained from `channel_open_session`.  
**How to avoid:** Store `Channel<Msg>` in Handler state keyed by `ChannelId`.  
**Warning signs:** Client hangs after auth; empty pack.

### Pitfall 2: Quoting / path injection in exec
**What goes wrong:** `git-upload-pack '../../../etc'` escapes repos root.  
**How to avoid:** Strip quotes; split owner/name; `bare_repo_path`; never pass raw path to shell (`Command::new("git-upload-pack").arg(path)` — no `sh -c`).  
**Warning signs:** Absolute paths or spaces accepted.

### Pitfall 3: HTTP status muscle memory
**What goes wrong:** Private deny returns opaque disconnect without git-friendly message.  
**How to avoid:** Write clear stderr (`ERROR: Repository not found.` / `Permission denied` / verify email) then close channel — D-SSH-04.  
**Warning signs:** Users only see `fatal: Could not read from remote repository`.

### Pitfall 4: Compose port 2222 vs advertised URL
**What goes wrong:** Clone works in docs as port 22 but Compose listens 2222.  
**How to avoid:** Advertise `OXIDEAN_SSH_PORT`; document `Host`/`Port` in CloneBox how-to when ≠ 22.  
**Warning signs:** Smoke passes with `-p 2222` but UI omits Port hint.

### Pitfall 5: Host key regeneration every boot
**What goes wrong:** Clients get TOFU warnings continuously.  
**How to avoid:** Persist host key under volume-backed `OXIDEAN_SSH_HOST_KEY_DIR`; generate once if missing.  
**Warning signs:** `REMOTE HOST IDENTIFICATION HAS CHANGED` in smoke.

### Pitfall 6: Rate-limit counting `auth_publickey_offered`
**What goes wrong:** Honest clients probing keys burn limits.  
**How to avoid:** Count failures in `auth_publickey` reject path only; russh docs note offered should usually not count. [CITED: docs.rs russh Handler `auth_publickey_offered`]

### Pitfall 7: Enabling SSH on every `make dev`
**What goes wrong:** Port conflicts / unexpected listener.  
**How to avoid:** D-SSH-07 — Compose-first; gate with `OXIDEAN_SSH_ENABLED`.

## Code Examples

### Fingerprint (registration)

```rust
// Source: docs.rs/ssh-key PublicKey::fingerprint + Fingerprint Display
use ssh_key::{HashAlg, PublicKey};

fn fingerprint_openssh_line(line: &str) -> Result<String, ssh_key::Error> {
    let key = PublicKey::from_openssh(line.trim())?;
    // Display → "SHA256:…" (OpenSSH form)
    Ok(key.fingerprint(HashAlg::Sha256).to_string())
}
```

### Force user + accept key (sketch)

```rust
// Source: docs.rs/russh/0.63.3 server::Handler
async fn auth_publickey(
    &mut self,
    user: &str,
    public_key: &russh::keys::ssh_key::PublicKey, // exact path: follow russh 0.63 exports
) -> Result<Auth, Self::Error> {
    if user != "git" {
        return Ok(Auth::Reject);
    }
    // lookup fingerprint in DB; Accept + stash user_id on handler
    Ok(Auth::Accept)
}
```

### Pack spawn (conceptual)

```rust
// Source: russh discussion #313 + Channel::make_reader/make_writer docs
// 1) channel_success(id)
// 2) Command::new("git-upload-pack").arg(bare_path)
//      .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
// 3) copy between channel reader/writer and child stdio (incl. stderr → extended data)
// 4) wait; channel EOF / exit-status
```

### Smart HTTP ACL helpers to call

```rust
// Source: crates/oxidean-api/src/repo/acl.rs
pub fn is_private_visibility(visibility: &str) -> bool { /* … */ }
pub fn can_read_as_owner(caller_user_id: Option<&str>, owner_id: &str) -> bool { /* … */ }
```

### Rate-limit constants to mirror (discretion)

```rust
// Source: crates/oxidean-api/src/pat/rate_limit.rs:9-11
const WINDOW: Duration = Duration::from_secs(15 * 60);
const IP_LIMIT: usize = 20;
const USER_LIMIT: usize = 10; // for SSH: fingerprint bucket replaces user id
```

**Recommendation:** Reuse **20 / IP / 15m** and **10 / fingerprint / 15m**; clear fingerprint bucket on success (same as `clear_user`).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| OpenSSH + git-shell / AuthorizedKeysCommand | Application SSH (russh/Gitea-style) | Forge practice | Identity tied to app DB |
| thrussh | russh (maintained fork lineage) | ~2022+ | Use russh 0.63 |
| Anonymous public HTTPS fetch | SSH always authenticated | forge norm | Still require registered key for public clone over SSH |

**Deprecated/outdated:**
- `thrussh` as primary dependency (SUS / low downloads).
- Treating `rsa-sha2-*` as a fingerprint algorithm — it is a **signature** algorithm; fingerprint remains SHA256 of key blob.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Anonymous SSH clone of public repos is **out of scope**; public fetch requires a registered key (reading of D-SSH-04) | ACL | Product may want anonymous SSH later — would need CONTEXT amend |
| A2 | Rejecting `git-upload-archive` in v1 is acceptable | Pack allowlist | Archive-over-SSH deferred; HTTPS archives remain |
| A3 | Hard-delete on revoke (no soft revoke) is preferred | Schema | If audit needs history, add `revoked_at` |
| A4 | Host key: single Ed25519 host key file is enough for v1 | Ops | Some clients prefer RSA host key too — add later if needed |
| A5 | `ssh-key` 0.7.0-rc.11 remains russh’s pin through implementation | Stack | If russh bumps, re-pin together |

**If this table is empty:** (not empty — confirm A1 with planner if product wants GitHub-like “any key works for public” only when authenticated — already assumed yes.)

## Open Questions (RESOLVED)

1. **Anonymous public SSH?** — **RESOLVED:** Require registered key for all SSH; document HTTPS for anonymous public clone (D-SSH-04: public fetch OK when authenticated).

2. **Separate advertise vs listen port?** — **RESOLVED:** single `OXIDEAN_SSH_PORT` + docs (D-SSH-02). Revisit only if Railway/cloud forces different publish mapping in Phase 22.

3. **RPC naming** — **RESOLVED:** Locked at plan time as **`sshKey.add` / `sshKey.list` / `sshKey.revoke`** (session cookie; not PAT). RESEARCH earlier draft `ssh.listKeys`/`addKey`/`revokeKey` is superseded.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| Rust / cargo | API build | ✓ | rustc 1.100 nightly / cargo 1.100 | — |
| git | pack spawn + smoke | ✓ | 2.55.0 | — |
| OpenSSH client (`ssh`/`ssh-keygen`) | smoke-git-ssh | ✓ | OpenSSH_10.4p1 | — |
| Docker | Compose smoke | ✓ | 28.4.0 | skip smoke like HTTPS script |
| Bun | web tests | ✓ | 1.4.0 | — |
| ctx7 CLI | docs lookup | ✗ | — | WebFetch docs.rs (used) |
| Context7 MCP | docs lookup | ✗ | — | WebFetch / crates.io |

**Missing dependencies with no fallback:** none for planning/execution on this host.  
**Missing dependencies with fallback:** Context7 → docs.rs WebFetch.

## Validation Architecture

> Nyquist enabled (`workflow.nyquist_validation: true` in `.planning/config.json`).

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo nextest (Rust) + Vitest (web) |
| Config file | workspace Cargo / `apps/web/vitest.config.ts` |
| Quick run command | `cargo nextest run -p oxidean-api -E 'test(ssh) or test(git_ssh)'` + `bunx vitest run src/routes/settings/ssh-keys.integration.test.ts src/components/repo/clone-box.integration.test.ts` (cwd `apps/web`) |
| Full suite command | `make test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GIT-04 | add/list/revoke keys; max 25; require_verified; unique fingerprint | unit/integration | nextest `ssh_keys` / dialect migration tests | ❌ Wave 0 |
| GIT-04 | `/settings/ssh-keys` list + confirm revoke | integration | vitest settings ssh-keys | ❌ Wave 0 |
| GIT-03 | force user `git`; reject other usernames | unit | nextest ssh auth | ❌ Wave 0 |
| GIT-03 | public fetch with key; private non-owner denied (git error) | integration | nextest ssh pack ACL | ❌ Wave 0 |
| GIT-03 | push requires verified email | integration | nextest | ❌ Wave 0 |
| GIT-03 | only upload/receive-pack; reject shell | unit | nextest | ❌ Wave 0 |
| GIT-03 | Compose TCP + ls-remote/push | smoke | `make smoke-git-ssh` | ❌ Wave 0 |
| GIT-03/04 | CloneBox SSH URL + CTA | integration | vitest clone-box | ⚠️ exists but placeholder — update |

### Sampling Rate

- **Per task commit:** targeted nextest filter + relevant vitest file  
- **Per wave merge:** `make test`  
- **Phase gate:** `make test` + `make smoke-git-ssh` (Compose) + `make rpc-sync-check`

### Wave 0 Gaps

- [ ] `crates/oxidean-api/tests/git_ssh.rs` (or colocated) — RED: auth user, ACL, pack allowlist, rate-limit stubs  
- [ ] `crates/oxidean-db` dialect tests for `0009_ssh_keys`  
- [ ] `apps/web/src/routes/settings/ssh-keys.integration.test.ts` — RED route stubs  
- [ ] `scripts/smoke-git-ssh.sh` + Makefile target  
- [ ] Update `clone-box.integration.test.ts` expectations once SSH live  

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | Public-key auth only; reject password/none; force user `git` |
| V3 Session Management | no (SSH) / yes (RPC) | SSH: no cookie; RPC key CRUD: opaque session cookies |
| V4 Access Control | yes | Shared owner-only ACL; Phase 10-ready helpers |
| V5 Input Validation | yes | OpenSSH parse; path allowlist; title length; max 25 keys |
| V6 Cryptography | yes | russh + host keys; fingerprint SHA-256; no hand-rolled crypto |

### Known Threat Patterns for git-over-SSH

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Path traversal via exec arg | Tampering | `bare_repo_path`; no shell |
| Interactive shell / port-forward | Elevation | Reject shell/pty/subsystem/tcpip |
| Auth brute / key spray | DoS | Rate-limit IP + fingerprint; russh `auth_rejection_time` |
| Private repo enumeration | Info disclosure | Clear but non-leaky git errors (align with forge norms; prefer identical “not found” for missing vs denied when feasible) |
| Host key swap | Spoofing | Persist host keys on volume; document fingerprint for operators |
| Unverified push | Elevation | Block receive-pack if `email_verified_at` null |
| SSH as PAT-scope bypass | Elevation | Document keys = full account; no FG scopes (product decision) |

## Sources

### Primary (HIGH confidence)

- In-repo: `09-CONTEXT.md`, `08-CONTEXT.md`, `git_smart_http.rs`, `repo/acl.rs`, `http_backend.rs`, `pat/rate_limit.rs`, `auth_types.rs` reserved list, `clone-box.tsrx`, `settings-nav.tsrx`, `docker-compose.yml`, `scripts/smoke-git-https.sh`, `docs/CONFIGURATION.md`
- crates.io: `russh` 0.63.3; dependency `ssh-key =0.7.0-rc.11`
- docs.rs: `russh` 0.63.3 `Handler` / `Channel`; `ssh-key` Fingerprint / `PublicKey::fingerprint`

### Secondary (MEDIUM confidence)

- GitHub Eugeny/russh discussion #313 — pack stdio bridging
- Upsilon blog “Hallo ssh” — forge russh + pack commands overview
- OpenSSH fingerprint community writeups (SHA256 + strip `=`) cross-checked with ssh-key Display example

### Tertiary (LOW confidence)

- Exact stderr copy strings for deny/unverified (product polish — set in plan/UI-SPEC)

## Metadata

**Confidence breakdown:**
- Standard stack: **HIGH** — crates.io + legitimacy OK + locked D-SSH-01
- Architecture: **HIGH** — mirrors Phase 8 ACL + bare paths; russh Handler surface verified on docs.rs
- Pitfalls: **MEDIUM–HIGH** — channel lifetime / quoting / host keys well-known; exact russh 0.63 Handler signature details (Channel vs ChannelId in exec) must be confirmed against the pinned crate during Wave 0 spike

**Research date:** 2026-09-14  
**Valid until:** ~2026-10-14 (russh/ssh-key move quickly — re-check versions at plan execute)
