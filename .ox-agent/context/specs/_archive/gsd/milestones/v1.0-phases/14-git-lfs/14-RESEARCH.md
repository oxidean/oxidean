# Phase 14: Git LFS - Research

**Researched:** 2026-09-14
**Domain:** Git LFS Batch API + basic transfer; Axum Smart HTTP auth; volume-backed OID store; Octane settings/blob UI
**Confidence:** HIGH (protocol + in-repo seams); MEDIUM (quota/GC defaults) — D-LFS-07 multipart interpretation **RESOLVED** (basic + streaming PUT + Range/verify)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
#### A — Storage layout
- **D-LFS-01:** Separate instance volume via **`OXIDEAN_LFS_DIR`** (Compose bind like uploads), not under each bare repo — **Reversibility:** costly — ops path + backup story
- **D-LFS-02:** **Instance-wide content-addressed OID store** (dedup across repos) — **Reversibility:** costly — shared GC/refcount model
- **D-LFS-03:** On-disk layout **OID-sharded** (`ab/cd/<oid>`) plus **DB pointer/refcount rows** — **Reversibility:** costly — migration if changed
- **D-LFS-04:** **Factory reset wipes `LFS_DIR`** along with repositories — **Reversibility:** reversible (policy)

#### B — Transport
- **D-LFS-05:** **HTTPS only** in Phase 14 (batch + content transfer); **LFS-over-SSH deferred** — **Reversibility:** reversible (add later)
- **D-LFS-06:** Routes under **`/{owner}/{repo}.git/info/lfs/…`** (reuse `.git` Traefik → API routing) — **Reversibility:** costly — client URL expectations
- **D-LFS-07:** Ship **basic transfer** as GIT-12 gate + **resumable-within-basic** (streaming PUT, optional verify, Range GET); **no** `transfer=multipart` adapter — **Reversibility:** reversible — **Locked deviation:** discuss said “multipart/resumable”; Phase 14 locks RESEARCH client-reality interpretation (official multipart is proposal-only)
- **D-LFS-08:** **Git clone without LFS smudge remains valid** (pointer files in tree) — GitHub/Gitea parity — **Reversibility:** reversible

#### C — Auth & ACL
- **D-LFS-09:** Auth matches Smart HTTP: **PAT Basic**; **Read** for download, **Write** for upload; **no session cookies** for LFS — **Reversibility:** reversible
- **D-LFS-10:** **Per-repo enable/disable**; only **Admin** may toggle — **Reversibility:** reversible
- **D-LFS-11:** **Reuse existing contents/repo PAT scopes** — no dedicated `lfs` fine-grained permission — **Reversibility:** reversible

#### D — Operator & limits
- **D-LFS-12:** **Configurable max object size** and **per-repo + per-user storage quotas** in Phase 14 — **Reversibility:** costly — quota schema + enforcement
- **D-LFS-13:** **Instance env defaults** with **Admin UI overrides** — **Reversibility:** reversible
- **D-LFS-14:** **Reject** uploads that exceed max size or quota (clear error; no soft-warn-only) — **Reversibility:** reversible
- **D-LFS-15:** **Refcount + periodic GC** of unreferenced OIDs — **Reversibility:** costly — GC job + safety

#### E — Client UX
- **D-LFS-16:** Ship **docs + repo Settings** (toggle/status) **+ pointer badges on blob + in-app LFS browser / quota dashboards** (tree → blob; no separate tree-row badge) — **Reversibility:** costly — UI surface area
- **D-LFS-17:** **Document `.gitattributes` patterns only** — no server auto-commit of attributes — **Reversibility:** reversible
- **D-LFS-18:** Blob view **detects LFS pointer** and offers **Download via LFS** (AuthZ = Read) — **Reversibility:** reversible
- **D-LFS-19:** Usage dashboards at **repo settings** (this repo) and **Admin** (instance); both include **usage breakdown** — **Reversibility:** reversible

### Claude's Discretion
- Exact default max object size and default quota numbers
- Exact streaming/chunk buffering for basic PUT and Range GET windowing (within locked D-LFS-07 — not multipart adapter design)
- Exact GC schedule and locking
- Exact breakdown dimensions (by repo, user, OID count, bytes) as long as both dashboards show a useful breakdown
- Exact Settings copy for enable LFS + link to docs

### Deferred Ideas (OUT OF SCOPE)
- LFS-over-SSH
- S3 / external object stores
- Auto-commit starter `.gitattributes`
- LFS File Locking API
- Official `transfer=multipart` adapter (await stock client)
- Rejecting git clone when LFS objects missing (non-parity)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| GIT-12 | User can push and fetch Git LFS objects for a repository | Batch API + basic transfer under `/{owner}/{repo}.git/info/lfs`; PAT Basic + Capability Read/Write; per-repo enable gate |
| GIT-13 | Operator can configure LFS storage on the filesystem (volume-backed) for the instance | `OXIDEAN_LFS_DIR` Compose bind; AppState `lfs_dir`; CONFIGURATION docs; factory-reset wipe |
</phase_requirements>

## Summary

Phase 14 adds a first-class Git LFS server to Oxidean on the existing Smart HTTP `.git` surface. Clients discover LFS at `{remote}.git/info/lfs` and call `POST …/objects/batch`, then transfer bytes with the **basic** adapter (GET download / PUT upload / optional verify). Auth must mirror `git_smart_http.rs`: PAT Basic only, cookies ignored, Read for download, Write + verified email for upload, classic `repo` / FG `contents` scopes (no new `lfs` scope). Storage is a separate volume (`OXIDEAN_LFS_DIR`) with instance-wide OID dedup, DB refcounts, quotas, and GC — matching Gitea/Forgejo-shaped filesystem forges rather than per-bare-repo `.git/lfs`.

**Critical planning constraint for D-LFS-07 (LOCKED 2026-09-14):** the stock `git-lfs` client (verified locally `git-lfs/3.7.1`; official API README) currently documents **only `basic`** as a supported transfer adapter. The `multipart` transfer mode is a **proposal** (`docs/proposals/multipart_transfer_mode.md`) and is **not** implemented in the open-source client. CONTEXT D-LFS-07 is amended to match: Batch + basic is the GIT-12 path; “multipart/resumable” means streaming PUT + optional verify + Range GET — **not** requiring clients to negotiate `transfer: "multipart"`.

**Primary recommendation:** Implement Axum LFS routes beside Smart HTTP; reuse `authenticate_pat` / `effective_capability` / `pat_allows_operation`; store objects at `{OXIDEAN_LFS_DIR}/{oid[0:2]}/{oid[2:4]}/{oid}`; enforce enable + quotas before issuing upload actions; raise/disable Axum body limits on LFS PUT; extend factory reset + Compose; ship Octane Settings/Admin/blob/browser UI via RPC (`make rpc-gen`).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| LFS Batch + object transfer | API / Backend | — | Protocol lives on Axum under `.git/info/lfs`; not SPA |
| PAT auth + ACL for LFS | API / Backend | — | Same as Smart HTTP; cookies must not auth |
| OID filesystem store | Database / Storage | API / Backend | Volume + DB refcount/quota rows; API streams I/O |
| Per-repo LFS enable toggle | API / Backend | Browser / Client | Admin Capability via RPC; Settings UI |
| Quotas / usage dashboards | API / Backend | Browser / Client | Enforcement server-side; Admin + repo Settings display |
| Pointer badge + Download | Browser / Client | API / Backend | Detect pointer in blob RPC; download via LFS/session-safe helper |
| Compose `OXIDEAN_LFS_DIR` | CDN / Static (ops) | API / Backend | Volume bind + env; Traefik already routes `.git` |
| Docs / `.gitattributes` guidance | Browser / Client | — | Docs-only; no auto-commit |

## Project Constraints (from `.cursor/rules/`)

- One product: Bun workspaces + Cargo crates; do not invent parallel app structure. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]
- Web UI is **Octane** (`.tsrx`), not React JSX; load Octane skill before UI edits. [VERIFIED: `.cursor/rules/octane-ui.mdc`]
- RPC types: change Rust → `make rpc-gen`; never hand-edit `@oxidean/api-client` as SoT. [VERIFIED: `.cursor/rules/rpc-codegen.mdc`]
- Dialect SQL only inside `crates/oxidean-db`. [VERIFIED: `.cursor/rules/rust-crates.mdc`]
- Prefer extending Smart HTTP / ACL / Make smoke patterns over new frameworks. [VERIFIED: `AGENTS.md`]

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Axum (existing) | workspace | LFS HTTP routes | Already serves Smart HTTP [VERIFIED: `crates/oxidean-api/src/app.rs:140-152`] |
| `sha2` | `0.11.0` (in-tree) | Hash upload stream vs OID | Already depended; verify OID on PUT [VERIFIED: `crates/oxidean-api/Cargo.toml`] |
| `tempfile` | `3` (in-tree) | Atomic write via `.tmp` + rename | Forgejo ContentStore pattern [CITED: Forgejo `content_store.go` via search] |
| system `git-lfs` (CI/smoke) | ≥3.x | Client smoke | Present in env: `git-lfs/3.7.1` [VERIFIED: local `git-lfs --version`] |
| Octane + TanStack Query | in-tree | Settings / Admin / blob UI | Project UI stack [VERIFIED: AGENTS.md] |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio::fs` / `tokio-util` | in-tree | Stream large bodies to disk | Avoid buffering whole OID in RAM |
| `tower_http::limit::RequestBodyLimitLayer` | via axum/tower | Outer body ceiling on LFS PUT | Pair with `DefaultBodyLimit::disable()` or `max(quota)` |
| Existing `FailedAuthLimiter` / PAT helpers | in-tree | Rate-limit failed Basic | Mirror Smart HTTP D-26 |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| In-process Axum LFS | External `lfs-test-server` / Giftless | Extra service; conflicts with one-API Compose story |
| `multipart` transfer adapter | Stock `basic` only | Multipart not in open-source client; basic is required for GIT-12 |
| Per-repo `.git/lfs` | Instance `OXIDEAN_LFS_DIR` | Locked D-LFS-01/02; per-repo breaks dedup/GC |

**Installation:** No new npm packages required for Phase 14. Reuse existing Rust crates; do not add LFS JS SDKs.

**Version verification:** `sha2` / `tempfile` / `bytes` / `tokio` legitimacy `OK` via `gsd_run query package-legitimacy check --ecosystem crates` [VERIFIED: gsd package-legitimacy].

## Package Legitimacy Audit

> Phase installs **no new external packages** if planner sticks to in-tree deps.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| sha2 | crates | since 2016 | ~19M/wk | github.com/RustCrypto/hashes | OK | Already in workspace — Approved |
| tempfile | crates | since 2015 | ~14M/wk | github.com/Stebalien/tempfile | OK | Already in workspace — Approved |
| bytes | crates | since 2015 | ~19M/wk | github.com/tokio-rs/bytes | OK | Already in workspace — Approved |
| tokio | crates | since 2016 | ~17M/wk | github.com/tokio-rs/tokio | OK | Already in workspace — Approved |

**Packages removed due to [SLOP] verdict:** none  
**Packages flagged as suspicious [SUS]:** none  

*If planner later proposes a new crate (e.g. tus server), re-run legitimacy before install.*

## Architecture Patterns

### System Architecture Diagram

```
git-lfs client
    │  POST /{owner}/{repo}.git/info/lfs/objects/batch
    │  Accept: application/vnd.git-lfs+json
    ▼
Traefik PathRegexp `^/[^/]+/[^/]+\.git` (prio 110)
    ▼
Axum LFS routes (new) ──► authenticate_pat (Cookie ignored)
    │                      effective_capability + pat_allows_operation
    │                      repo.lfs_enabled? quotas? max size?
    ▼
Batch response (transfer=basic)
    ├─ download → GET  …/objects/{oid}     → stream from LFS_DIR
    ├─ upload   → PUT  …/objects/{oid}     → tmp → sha256 verify → rename
    └─ verify   → POST …/objects/verify    → size/oid check (optional but recommended)
    ▼
OXIDEAN_LFS_DIR / ab / cd / <oid>
    ▲
DB: lfs_objects + lfs_object_links (refcount) + quota counters
    ▲
RPC (session): repo.lfs.* / admin.lfs.*  → Octane Settings / Admin / browser
```

### Recommended Project Structure

```
crates/oxidean-api/src/
├── routes/git_lfs.rs          # batch + basic transfer handlers
├── lfs/
│   ├── mod.rs
│   ├── store.rs               # path sharding, put/get/verify, wipe
│   ├── batch.rs               # JSON request/response types
│   └── auth.rs                # thin wrapper reusing Smart HTTP PAT/ACL
├── jobs/lfs_gc.rs             # periodic unreferenced OID GC
apps/web/src/
├── routes/$owner.$repo.settings.tsrx   # LFS toggle + repo usage
├── routes/admin/…                      # instance quotas/usage
├── components/repo/blob-viewer.tsrx    # pointer badge + Download
├── components/repo/lfs-browser.tsrx    # in-app OID browser
crates/oxidean-db/migrations/*/0011_lfs.sql   # or next free number at execute time
```

### Pattern 1: Batch then basic transfer

**What:** Client POSTs batch; server returns `href`s under the same origin; client GET/PUT raw bytes.  
**When to use:** All stock `git-lfs` clients.  
**Example:**

```json
// Source: https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md
// Pin revision for planners: commit 39cda42d875f6459760f6296890e4e8441eee25b (docs/api/batch.md path, 2025-04-21)
POST /{owner}/{repo}.git/info/lfs/objects/batch
Accept: application/vnd.git-lfs+json
Content-Type: application/vnd.git-lfs+json
Authorization: Basic …

{
  "operation": "upload",
  "transfers": ["basic"],
  "objects": [{ "oid": "<64-hex>", "size": 123 }]
}
```

If the server already has the OID linked for this repo, **omit `actions`** so the client skips upload. [CITED: git-lfs batch.md]

### Pattern 2: Mirror Smart HTTP auth matrix

**What:** Extract shared helpers from `git_smart_http.rs` (or call the same private modules after a small refactor) for LFS.  
**When to use:** Every LFS endpoint including object GET/PUT (actions may set `authenticated: true` and still require Basic).  
**AuthZ mapping:**

| Operation | Capability | PAT | Email verified |
|-----------|------------|-----|----------------|
| download (public) | anonymous OK | — | — |
| download (private) | Read | classic `repo` or FG contents read/write | not required |
| upload | Write | classic `repo` or FG contents **write** | required (mirror receive-pack) |
| enable toggle RPC | Admin | session RPC (not PAT) | privileged gates as today |

[VERIFIED: `crates/oxidean-api/src/routes/git_smart_http.rs:289-354` scopes; `:428-464` receive-pack gates]  
[VERIFIED: `crates/oxidean-api/src/repo/acl.rs:24-28` — `Capability { Read = 1, Write = 2, Admin = 3 }`]

### Pattern 3: OID store + refcount

**What:** Global content-addressed files + DB rows linking `(repository_id, oid)`.  
**On-disk path (locked D-LFS-03):** `{LFS_DIR}/{oid[0:2]}/{oid[2:4]}/{oid}` — same shard as Forgejo server layout (`OID[0:2]/OID[2:4]/OID[4:]` with full oid as filename). [CITED: Forgejo ContentStore `transformKey`]  
**Note:** Client local `.git/lfs/objects` uses `OID[0:2]/OID[2:4]/OID` (filename = full oid too). Do not share client and server directories. [CITED: git-lfs spec.md]

### Anti-Patterns to Avoid

- **Buffering entire LFS bodies in `Bytes`:** Axum’s default 2 MiB `DefaultBodyLimit` will reject large PUTs; stream to disk. [CITED: docs.rs axum `DefaultBodyLimit`]
- **Authenticating LFS with session cookies:** Forbidden by D-LFS-09 / Phase 8 D-12.
- **Returning HTTP 401 for insufficient PAT scope:** Use **403** (Smart HTTP D-23).
- **Using `WWW-Authenticate` alone on LFS JSON 401:** Prefer **`LFS-Authenticate: Basic realm="…"`** so browsers don’t steal the prompt. [CITED: git-lfs batch.md]
- **Implementing only `multipart` transfer:** Stock clients never select it → GIT-12 fails.
- **Auto-committing `.gitattributes`:** Deferred / locked out (D-LFS-17).
- **Hand-editing `packages/api-client`:** Use `make rpc-gen`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| LFS wire protocol | Invent custom binary protocol | Official Batch + basic transfer | Client compatibility |
| Auth for git/LFS | New token type | Existing PAT Basic + ACL | D-LFS-09/11 |
| Content hashing | Custom hash | SHA-256 OID from stream (`sha2`) | Spec requires sha256 |
| Atomic object write | Direct overwrite | temp file + fsync + rename | Corruption / partial PUT |
| UI framework | React/Zustand for LFS pages | Octane `.tsrx` + Query | Project rules |
| Dialect SQL in API | `#[cfg]` SQL | `oxidean-db` migrations/API | Hard boundary |

**Key insight:** LFS is an HTTP JSON+bytes protocol bolted beside Smart HTTP — reuse auth/ACL/routing; invent only the OID store + quota/GC policy.

## Common Pitfalls

### Pitfall 1: Axum 2 MiB body limit
**What goes wrong:** Uploads >2 MiB fail before handler runs.  
**Why:** `Bytes`/`Json` extractors apply default limit.  
**How to avoid:** On LFS PUT routes, `DefaultBodyLimit::disable()` + `RequestBodyLimitLayer` at max-object (or stream body without limited extractors) and enforce size while hashing.  
**Warning signs:** Client 413 on modest LFS files; avatar route already uses explicit `DefaultBodyLimit::max` [VERIFIED: `app.rs:127-129`].

### Pitfall 2: Multipart adapter ≠ GIT-12
**What goes wrong:** Plan spends waves on `multipart` transfer; `git lfs push` still fails.  
**Why:** Official client only supports `basic` today. [CITED: git-lfs `docs/api/README.md`; issue #5413]  
**How to avoid:** Make basic Batch+PUT/GET the phase gate; treat multipart as optional/experimental add-on or resumable-within-basic.

### Pitfall 3: Quota vs dedup accounting
**What goes wrong:** Shared OID counted twice on disk or not counted on second repo.  
**How to avoid:** Separate **physical bytes** (unique OIDs) from **logical bytes** (sum of linked sizes per repo/user). Enforce upload against logical quotas; GC uses physical refcount==0.

### Pitfall 4: SSH remotes still need HTTPS LFS creds
**What goes wrong:** `git@host:owner/repo.git` clones work but LFS smudge fails auth.  
**Why:** Without `git-lfs-authenticate` (deferred with LFS-over-SSH), client falls back to guessed HTTPS LFS URL and Git credentials. [CITED: server-discovery.md]  
**How to avoid:** Document PAT credential helper for LFS HTTPS even when git remote is SSH; do not claim SSH-only LFS in Phase 14.

### Pitfall 5: Traefik / route ordering
**What goes wrong:** LFS hits SPA HTML.  
**How to avoid:** Existing PathRegexp already covers `*.git` including `/info/lfs`. Mount Axum routes **before** any catch-alls; path param `{repo_git}` must keep `.git` suffix like Smart HTTP. [VERIFIED: `docker-compose.yml` api-git rule; `app.rs:140-152`]

### Pitfall 6: Disabled LFS repo
**What goes wrong:** Ambiguous 404 vs batch errors.  
**How to avoid:** When `lfs_enabled=false`, return LFS JSON error (recommend **404** repository/feature unavailable or **403** with clear message) consistently for batch and object routes; Settings must show disabled state.

### Pitfall 7: Factory reset scope
**What goes wrong:** DB wiped but LFS volume retains orphans (or opposite).  
**How to avoid:** Extend `database_and_repositories` to also wipe `OXIDEAN_LFS_DIR` children (D-LFS-04), mirroring `wipe_repos_dir_contents`. [VERIFIED: `admin.rs:210-212` repos wipe today]

## Code Examples

### LFS pointer detection (blob UI)

```text
# Source: https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md
version https://git-lfs.github.com/spec/v1
oid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393
size 12345
```

Rules: UTF-8; `< 1024` bytes; first line `version …`; `oid sha256:` + 64 lowercase hex; `size` integer; single spaces. [CITED: git-lfs spec.md]  
Extend `BlobViewer` when `!is_binary && encoding===utf-8` and content matches — show badge + “Download” that fetches object bytes with Read auth (session RPC proxy or same-origin LFS GET with user PAT is **not** for browser — prefer **session-authenticated RPC/raw download** for UI Download, separate from git-lfs client PAT path). [ASSUMED: browser Download via session RPC is safer than embedding PAT]

### Capability ladder (verbatim)

```24:28:crates/oxidean-api/src/repo/acl.rs
pub enum Capability {
    Read = 1,
    Write = 2,
    Admin = 3,
}
```

### Smart HTTP route mount to mirror

```140:152:crates/oxidean-api/src/app.rs
        // Smart HTTP — D-18/D-22: only on /{owner}/{repo}.git (segment includes .git suffix)
        .route(
            "/{owner}/{repo_git}/info/refs",
            get(git_smart_http::info_refs),
        )
        .route(
            "/{owner}/{repo_git}/git-upload-pack",
            axum::routing::post(git_smart_http::upload_pack),
        )
        .route(
            "/{owner}/{repo_git}/git-receive-pack",
            axum::routing::post(git_smart_http::receive_pack),
        )
```

Add sibling routes, e.g. `…/info/lfs/objects/batch`, `…/info/lfs/objects/{oid}`, `…/info/lfs/objects/verify`.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| LFS objects inside bare repo | Instance content-addressed store | Forge practice (Gitea/Forgejo) | Dedup + shared GC (locked) |
| Only basic transfer | Experimental tus / proposed multipart | ongoing | Stock clients still basic-only |
| SSH `git-lfs-authenticate` | HTTPS LFS + optional SSH auth helper | LFS 3.x | Phase 14 HTTPS still serves SSH remotes’ LFS over HTTPS |

**Deprecated/outdated:**
- Assuming open-source `git-lfs` implements `multipart` transfer — it does not yet. [CITED: API README + #5413]

## Discretion Recommendations (planner defaults)

| Topic | Recommendation | Confidence |
|-------|----------------|------------|
| Max object size default | **2 GiB** (`OXIDEAN_LFS_MAX_OBJECT_BYTES=2147483648`) | [ASSUMED] GitHub-class default |
| Per-repo quota default | **10 GiB** logical | [ASSUMED] |
| Per-user quota default | **50 GiB** logical (sum of OIDs attributed to uploader) | [ASSUMED] |
| Unlimited sentinel | `0` or `-1` = unlimited in env/Admin | [ASSUMED] match Forgejo soft-quota style |
| Multipart/resumable (D-LFS-07) | **RESOLVED / LOCKED:** Ship basic + streaming PUT + optional `verify` + `Range` on GET. Do **not** ship or require `transfer=multipart`. Official multipart adapter deferred until stock clients support it. | HIGH (client reality + CONTEXT lock 2026-09-14) |
| Chunk size | N/A for basic single PUT; if server-side staging chunks used internally, **8–32 MiB** | [ASSUMED] |
| GC schedule | Interval job like orphan reconcile; default **24h**; grace **7 days** after refcount=0 before delete; lockfile under `LFS_DIR/.gc.lock` | [ASSUMED] Forgejo-inspired |
| Dashboard breakdown | Repo: OID count, logical bytes, top paths/OIDs; Admin: by repo, by user, instance physical bytes, OID count | aligns D-LFS-19 |
| Settings copy | “Enable Git LFS for this repository” + link to docs on `.gitattributes` / `git lfs track` | discretion |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Default max object 2 GiB / repo 10 GiB / user 50 GiB | Discretion | Operator surprise; easy env override |
| A2 | Browser “Download via LFS” uses session RPC, not PAT Basic in browser | Code Examples | XSS/token leakage if wrong |
| A3 | Migration id `0011_lfs` if Phase 14 lands before later phase migrations | Structure | Collision — resolve at execute via next free number |
| A4 | Charge logical bytes per repo link; physical bytes for disk/GC | Pitfalls | Quota unfairness |
| A5 | File Locking API out of scope (CONTEXT Deferred Ideas) | Open Questions (RESOLVED) | Users expecting lock may be disappointed |

**If wrong:** Discuss-phase can adjust A1/A2/A5 before plan lock; A3 is execute-time bookkeeping.

## Open Questions (RESOLVED)

1. **Browser Download auth path** — **RESOLVED:** Session-gated `repo.lfs.download` (or equivalent session RPC / `/api/…` with Capability Read). Never cookie-auth `.git/info/lfs`. Matches plans 14-08/14-11 ASSUME A2.

2. **Upload attribution for per-user quota** — **RESOLVED:** Enforce **both** repo logical quota and **repo owner** user quota; attribute new OID to **pushing user** for breakdown. Matches plan 14-04 ASSUME / RESEARCH A1–A4.

3. **LFS File Locking API** — **RESOLVED (deferred):** Explicitly out of Phase 14; listed under CONTEXT Deferred Ideas. Spec `docs/api/locking.md` not implemented.

4. **Migration number vs parallel phases** — **RESOLVED:** Use **next free** tri-dialect migration id at execute time; do not hard-code `0011_lfs` if Phases 11–13 ship first. Matches Wave 0 / tracer ASSUME (RESEARCH A3).

5. **D-LFS-07 multipart vs basic** — **RESOLVED (locked deviation):** Phase 14 delivers basic Batch + streaming PUT + verify + Range GET as “multipart/resumable” intent. Official `transfer=multipart` is proposal-only in stock git-lfs — deferred. CONTEXT D-LFS-07 amended 2026-09-14; plans 14-02/14-05 cite the lock.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | Smart HTTP / smoke | ✓ | 2.55.0 | — |
| `git-lfs` | Client smoke / e2e | ✓ | 3.7.1 | Install in CI image if missing |
| `docker` | Compose volume / Traefik smoke | probe at execute | — | Skip smoke exit 0 pattern |
| Rust / nextest | Unit/integration | ✓ | rustc nightly 1.100 | `cargo test` fallback |
| Node/Bun + Vitest | UI tests | ✓ | node v24.5.0 | — |
| Postgres/MySQL/SQLite | Migrations | dialect dirs present | — | All three migrations required |

**Missing dependencies with no fallback:** none identified for planning  
**Missing dependencies with fallback:** `git-lfs` on minimal CI images — install or skip LFS smoke like other smokes

Step 2.6: completed (external tools: git, git-lfs, Docker Compose volume).

## Validation Architecture

> `workflow.nyquist_validation` is true in `.planning/config.json`.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo-nextest (Rust) + Vitest ^5 (web) |
| Config file | `.config/nextest.toml`; `apps/web/vitest.config.ts` |
| Quick run command | `cargo nextest run -p oxidean-api -E 'test(lfs)'` (once named) |
| Full suite command | `make test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GIT-12 | Batch download/upload with PAT | integration | `cargo nextest run -p oxidean-api -E 'test(lfs_batch)'` | ❌ Wave 0 |
| GIT-12 | Reject upload without Write / unverified | integration | same | ❌ Wave 0 |
| GIT-12 | Skip actions when OID exists | unit | `cargo nextest run -p oxidean-api -E 'test(lfs_dedup)'` | ❌ Wave 0 |
| GIT-12 | Disabled repo rejects LFS | integration | `… lfs_disabled` | ❌ Wave 0 |
| GIT-13 | Objects land under `OXIDEAN_LFS_DIR` shard | integration | `… lfs_store_layout` | ❌ Wave 0 |
| GIT-13 | Factory reset wipes LFS_DIR | integration | extend `factory_reset_scope` | ❌ Wave 0 |
| GIT-12 | `git lfs push/pull` smoke over Traefik | smoke | `make smoke-git-lfs` (new) | ❌ Wave 0 |
| D-LFS-16 | Pointer badge detection | unit (web) | `bun run test:unit` blob/lfs pointer | ❌ Wave 0 |
| D-LFS-10 | Admin-only enable RPC | integration | `… lfs_enable_admin` | ❌ Wave 0 |
| D-LFS-14 | Over-quota → clear LFS error (507/422) | integration | `… lfs_quota` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** targeted nextest filter + relevant Vitest project  
- **Per wave merge:** `make test`  
- **Phase gate:** full suite + `make smoke-git-lfs` when Docker available  

### Wave 0 Gaps

- [ ] `crates/oxidean-api/tests/lfs_batch.rs` — GIT-12 batch/auth matrix  
- [ ] `crates/oxidean-api/tests/lfs_store.rs` — GIT-13 layout + verify hash mismatch  
- [ ] Extend `factory_reset_scope.rs` — LFS_DIR wipe  
- [ ] `scripts/smoke-git-lfs.sh` + Makefile target  
- [ ] `apps/web` unit tests for pointer parse helper  
- [ ] Framework: none new — reuse nextest/Vitest  

## Security Domain

> `security_enforcement` enabled; ASVS level 1.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | PAT Basic only on LFS; `LFS-Authenticate`; no cookies |
| V3 Session Management | no (LFS path) | Sessions stay on RPC/UI only |
| V4 Access Control | yes | `Capability` Read/Write/Admin; PAT scopes; private→401 |
| V5 Input Validation | yes | OID hex validation; size≥0; path traversal reject on shard join |
| V6 Cryptography | yes | SHA-256 verify on upload; no custom crypto |

### Known Threat Patterns for Git LFS / forge storage

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Upload as another repo’s OID without Write | Elevation | ACL + repo enable before link; never trust client alone |
| Path traversal in oid (`../`) | Tampering | Strict `^[a-f0-9]{64}$` before join |
| Filling disk via huge PUT | Denial | Max object + quotas; reject with 507/422; stream + abort |
| Cookie session used as git LFS auth | Spoofing | Ignore Cookie on `.git/info/lfs` |
| Timing/enumeration private repo | Information | Match Smart HTTP: unauth private → 401 Basic |
| Partial file left world-readable | Information | `0640`/`0750` like Forgejo; tmp outside final path |
| GC deletes live OID | Tampering | Refcount + grace + lock; only delete refcount=0 |

## Sources

### Primary (HIGH confidence)

- Git LFS Batch API — https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md (path commit `39cda42d875f6459760f6296890e4e8441eee25b`)
- Basic transfers — https://github.com/git-lfs/git-lfs/blob/main/docs/api/basic-transfers.md
- API README (adapters) — https://github.com/git-lfs/git-lfs/blob/main/docs/api/README.md
- Server discovery / SSH fallback — https://github.com/git-lfs/git-lfs/blob/main/docs/api/server-discovery.md
- Authentication — https://github.com/git-lfs/git-lfs/blob/main/docs/api/authentication.md
- Pointer spec — https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md
- In-repo: `git_smart_http.rs`, `acl.rs`, `app.rs`, `admin.rs` factory reset, `docker-compose.yml`, `docs/CONFIGURATION.md`, `docs/API.md`

### Secondary (MEDIUM confidence)

- Multipart proposal — https://github.com/git-lfs/git-lfs/blob/main/docs/proposals/multipart_transfer_mode.md
- git-lfs issue #5413 (multipart client not landed)
- Gitea issue #4944 / Forgejo ContentStore + soft-quota docs (filesystem layout + quotas)

### Tertiary (LOW confidence)

- Exact default quota numbers (discretion / [ASSUMED])

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — reuse in-tree Axum/sha2/Octane; no new packages
- Architecture: HIGH — locked CONTEXT + official LFS HTTPS discovery path
- Pitfalls: HIGH — body limit, multipart client gap, SSH credential UX verified against docs
- Defaults (quotas/GC): MEDIUM — need discuss confirmation of A1

**Research date:** 2026-09-14  
**Valid until:** 2026-10-14 (re-check git-lfs multipart client status if still pursuing adapter)

## RESEARCH COMPLETE

**Phase:** 14 - git-lfs  
**Confidence:** HIGH  

### Key Findings

1. Stock Git LFS clients only speak **Batch + basic** transfer; `multipart` is a proposal — plan basic as the GIT-12 gate.
2. Reuse Smart HTTP PAT/ACL/Traefik `.git` routing; add Axum LFS routes under `info/lfs`; raise body limits and stream to `OXIDEAN_LFS_DIR/{ab}/{cd}/{oid}`.
3. SSH git remotes still use **HTTPS LFS** via discovery; document credential-helper UX (no `git-lfs-authenticate` in Phase 14).
4. Dedup needs refcount + dual quota accounting (logical vs physical); factory reset must wipe LFS volume with repos.
5. Rich UI (Settings toggle, Admin quotas, pointer badge, browser) is in-scope via session RPC + Octane — separate from PAT LFS wire path.

### File Created

`.planning/phases/14-git-lfs/14-RESEARCH.md`

### Confidence Assessment

| Area | Level | Reason |
|------|-------|--------|
| Standard Stack | HIGH | No new packages; Axum + sha2 + existing patterns |
| Architecture | HIGH | Locked decisions + official discovery/batch docs |
| Pitfalls | HIGH | Body limit + multipart client gap + SSH/HTTPS LFS UX |

### Open Questions (RESOLVED)

- Browser Download via session RPC (A2) — **RESOLVED**
- Per-user quota: enforce owner quota + attribute pusher (14-04) — **RESOLVED**
- File Locking API — **RESOLVED** deferred
- Migration numbering — **RESOLVED** next-free at execute
- D-LFS-07 multipart — **RESOLVED** locked as basic + streaming PUT + Range/verify (no multipart adapter)

### Ready for Planning

Research complete. Planner can now create PLAN.md files.
