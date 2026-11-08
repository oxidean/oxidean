# Phase 20: Packages Registry - Research

**Researched:** 2026-09-14
**Domain:** Multi-format package registry (OCI Distribution Spec + npm registry API + generic/raw) on a self-hosted forge
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
#### A — Scope & routing
- **D-PKG-01:** Registry on the **same host**, **path-based**: OCI `/v2/…`, npm `/npm/…`, generic `/generic/…` — **Reversibility:** costly — client config URLs
- **D-PKG-02:** Packages namespaced by **owner + name** (user or org); **optional link to a repository** — **Reversibility:** costly — ownership model
- **D-PKG-03:** Prefer **standard client URL layouts** for docker/npm/generic tools (not a single `/packages/{owner}`-only tree) — **Reversibility:** costly

#### B — Auth
- **D-PKG-04:** **Hybrid auth:** derive access from **owner ACL + existing PAT scopes**, **and** add fine-grained **`package:read` / `package:write`** — **Reversibility:** costly — token UX + ACL matrix
- **D-PKG-05:** **Anonymous pull** for **public** packages; auth required for private pulls and **all** publishes — **Reversibility:** reversible
- **D-PKG-06:** **Write+** on owning repo/org may **publish**; **Admin** may **delete** — **Reversibility:** reversible

#### C — Storage
- **D-PKG-07:** Separate **`OXIDEAN_PACKAGES_DIR`** volume (not LFS, not release-assets) — **Reversibility:** costly — ops/backup split
- **D-PKG-08:** **Content-addressed** blob store with **cross-package dedup** (especially OCI layers) — **Reversibility:** costly — GC/refcount
- **D-PKG-09:** **Max blob size + per-owner quotas** via env defaults + Admin UI; **reject** over-limit uploads — **Reversibility:** reversible

#### D — Lifecycle / UI
- **D-PKG-10:** **GitHub-aligned immutability:** OCI **digests immutable**, **tags mutable**; npm and generic **versions immutable** (no overwrite; delete to remove). No separate yank state — **Reversibility:** costly — format semantics
- **D-PKG-11:** UI: **owner packages page** + **repo-linked** package views when linked — **Reversibility:** reversible
- **D-PKG-12:** Delete version requires **type-to-confirm** (`name@version` / equivalent) — **Reversibility:** reversible

#### E — Format depth
- **D-PKG-13:** Ship **fully featured OCI + npm + generic/raw** in Phase 20 (all of PKG-01…03) — **Reversibility:** costly — large surface
- **D-PKG-14:** npm includes publish/install **plus** dist-tags, deprecate, and search — **Reversibility:** reversible (features additive)
- **D-PKG-15:** OCI includes push/pull manifests+blobs, tags, list, delete (GHCR-basic parity) — **Reversibility:** reversible
- **D-PKG-16:** Generic includes upload/download, list, delete with immutable version ids — **Reversibility:** reversible

### Claude's Discretion
- Exact OCI Distribution Spec version / endpoints subset required for docker/podman/buildah
- Exact npm registry API subset for modern npm/pnpm/yarn
- Generic auth header conventions and path scheme details
- Quota breakdown dimensions; GC schedule for unreferenced blobs
- Whether package visibility is always inherited from linked repo vs independent public/private flag when unlinked
- Cosign/provenance: include only if blocked on “fully featured OCI”; else defer

### Deferred Ideas (OUT OF SCOPE)
- Additional ecosystems (Maven, PyPI, NuGet, …)
- S3/remote blob backends
- Sigstore/cosign unless research proves required for stated OCI clients
- Cross-instance package replication
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PKG-01 | User can publish and pull OCI container images from an instance registry scoped to a repo or org | OCI Distribution Spec v1.1.1 `/v2/<owner>/<image>/…` endpoints + Docker token auth; Traefik `/v2` → API; ACL Write+/Admin |
| PKG-02 | User can publish and pull npm packages from an instance registry scoped to a repo or org | npm packument/publish/tarball/dist-tags under `/npm/{owner}/` registry root; PAT Bearer/Basic |
| PKG-03 | User can publish and pull generic/raw packages from an instance registry scoped to a repo or org | Gitea-shaped `/generic/{owner}/{name}/{version}/{file}` PUT/GET/DELETE; immutable versions |
| PKG-04 | Registry packages respect the same auth/visibility rules as their owning repo/org | Hybrid ACL (`Capability` ladder) + `package:read`/`package:write` PAT scopes; anonymous public pull |
| PKG-05 | User can list and delete package versions they are permitted to manage | Session RPC list/delete UI + registry DELETE endpoints; Admin for delete; type-to-confirm |
</phase_requirements>

## Summary

Phase 20 adds a same-host, path-prefixed multi-format registry to the existing Rust Axum API: OCI at `/v2`, npm at `/npm`, generic at `/generic`. Ownership is `owner + name` with optional repo link. Auth is hybrid — reuse Phase 10 `Capability` ACL (Write+ publish, Admin delete) and extend Phase 8 PATs with `package:read` / `package:write`. Blobs live in a new `OXIDEAN_PACKAGES_DIR` content-addressed store (LFS-like sharding/refcount, separate volume). Immutability matches the locked GitHub-aligned rule: OCI digests immutable / tags mutable; npm & generic versions no-overwrite (delete then republish allowed).

Research confirms docker/podman need Distribution Spec Pull+Push+Discovery+Management plus a Bearer token challenge (not cookies). Cosign/Referrers API is **not** required for basic push/pull — defer. npm needs packument GET, publish PUT with attachments, tarball GET, dist-tags, deprecate, and `/-/v1/search`. Generic should follow Gitea’s PUT/GET/DELETE file layout under Oxidean’s `/generic` prefix. Critical edge work: Traefik + Vite must route `/v2|/npm|/generic` to the API before the SPA; reserve usernames `v2`, `npm`, `generic`.

**Primary recommendation:** Implement three Axum protocol modules + shared content-addressed package blob store and package ACL helpers in-tree (extend existing `axum`/`sha2`/`uuid`); pin OCI Distribution Spec **v1.1.1** without Referrers; use per-owner npm registry URL `/npm/{owner}/`; defer cosign/referrers.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| OCI `/v2` protocol (push/pull/tags/delete) | API / Backend | CDN / Static — | Docker/podman speak HTTP to registry; must be Axum, not SPA |
| npm registry protocol | API / Backend | — | npm/pnpm/yarn hit registry host paths; session cookies unused |
| Generic/raw upload/download | API / Backend | — | curl/CI PUT/GET with PAT Basic |
| Package ACL + PAT scope checks | API / Backend | Database / Storage | Mirror `repo/acl.rs` Capability ladder + PAT intersection |
| Content-addressed blob persistence | Database / Storage | API / Backend | Files under `OXIDEAN_PACKAGES_DIR`; metadata/refcounts in DB |
| Owner/repo packages UI + type-to-confirm delete | Browser / Client | Frontend Server (SSR) | Octane `.tsrx` + TanStack Query; SSR loaders for lists |
| Admin quota/usage | Browser / Client | API / Backend | Admin RPC + ENV defaults pattern from LFS decisions |
| Edge routing `/v2|/npm|/generic` | CDN / Static (Traefik) | API / Backend | Must outrank SPA catch-all like `.git` PathRegexp |

## Project Constraints (from .cursor/rules/)

- One product: cloud + self-host; Bun workspaces + Cargo crates — no parallel app structure. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]
- Web UI is **Octane** (`.tsrx`), not React; load Octane skill before UI edits; TanStack Query for server data. [VERIFIED: `.cursor/rules/octane-ui.mdc`]
- RPC types: change Rust → `make rpc-gen`; never hand-edit `packages/api-client` as source of truth. [VERIFIED: `.cursor/rules/rpc-codegen.mdc`]
- DB dialects only inside `crates/oxidean-db`. [VERIFIED: `.cursor/rules/rust-crates.mdc`]
- No secrets in commits/examples; prefer `make test` / `make rpc-sync-check` / relevant e2e. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]
- Prefer extending existing patterns (Query session helpers, auth gates, Make targets) over new frameworks. [VERIFIED: `.cursor/rules/oxidean-core.mdc`]

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axum` | `0.8` (lock `0.8.9`) | HTTP routes for `/v2`, `/npm`, `/generic` + multipart uploads | Already API framework; multipart feature present [VERIFIED: `crates/oxidean-api/Cargo.toml:26`] |
| `tower-http` | `0.6` (lock `0.6.11`) | CORS/trace for registry clients | Existing stack [VERIFIED: `crates/oxidean-api/Cargo.toml:27`] |
| `sha2` | `0.11.0` declared (lock resolves `0.10.9`) | Content-address digests for blob store | Already used for PAT hashing patterns [VERIFIED: `crates/oxidean-api/Cargo.toml:39`] |
| `uuid` | `1.26.0` | OCI blob upload session IDs | Already depended [VERIFIED: `crates/oxidean-api/Cargo.toml:41`] |
| `tempfile` | `3` (lock `3.27.0`) | Staging incomplete uploads before digest commit | Already depended [VERIFIED: `crates/oxidean-api/Cargo.toml:49`] |
| `oxidean-db` / migrations | workspace | Package metadata, blob refcounts, quotas | Dialect SQL boundary [ASSUMED: next migration after `0010_orgs_acl`] |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `@octanejs/*` + TanStack Query | existing workspace | Owner/repo/Admin packages UI | List/delete/quota screens only — not protocol clients |
| Vitest `^5` / cargo-nextest | existing | Protocol unit + integration + smoke | Wave 0 RED → GREEN per forge convention |
| Traefik `v3.3` (Compose) | existing image | PathPrefix routers for registry | Edge must reach API before SPA [VERIFIED: `docker-compose.yml:7`] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| In-tree Axum OCI handlers | Embed CNCF `distribution` / Harbor | Huge ops surface; contradicts “extend existing patterns”; deferred |
| `/npm/{owner}/` registry root | Global `/npm/` + forced `@owner/pkg` only | Global registry fights multi-tenant ACL; per-owner URL matches Gitea client UX under locked `/npm` prefix |
| Docker Bearer token endpoint | Basic-only on `/v2` | Many clients expect Bearer challenge; Basic-only is fragile |
| Native Referrers API (OCI 1.1) | Defer; clients use referrers tag schema | Cosign not in scope; Pull/Push work without referrers |

**Installation:**
```bash
# No new npm packages required for Phase 20 protocol work.
# Rust: reuse workspace crates; only add a crates.io dep if a plan proves a gap
# (run package-legitimacy before any new crate).
```

**Version verification:** `axum`/`sha2`/`uuid`/`tempfile` confirmed via `Cargo.toml` + `Cargo.lock` this session. No new packages proposed for install.

## Package Legitimacy Audit

> No new external packages are recommended for Phase 20. Audit covers **reuse** of existing stack crates only.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| axum | crates | since 2021 | ~8.9M/wk | github.com/tokio-rs/axum | OK | Approved (already in tree) |
| sha2 | crates | since 2016 | ~19M/wk | github.com/RustCrypto/hashes | OK | Approved (already in tree) |
| uuid | crates | — | — | — | OK | Approved (already in tree) [ASSUMED: signals not re-fetched for uuid] |
| tempfile | crates | — | — | — | OK | Approved (already in tree) [ASSUMED] |
| digest | crates | since 2016 | ~21M/wk | github.com/RustCrypto/traits | OK | Optional only if needed; not required |
| hex | crates | since 2015 | ~11M/wk | github.com/KokaKiwi/rust-hex | OK | Optional only if needed; not required |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

*Do not introduce unverified registry server crates (e.g. random “oci-registry” crates from web search) without official docs + legitimacy gate.*

## Architecture Patterns

### System Architecture Diagram

```text
                    ┌─────────────────────────────┐
  docker/podman ───▶│ Traefik PathPrefix /v2       │──┐
  npm/pnpm/yarn ───▶│ Traefik PathPrefix /npm      │──┤
  curl generic ────▶│ Traefik PathPrefix /generic  │──┤
  Browser SPA ─────▶│ Traefik Host catch-all (web) │  │
                    └─────────────────────────────┘  │
                                                     ▼
                                         ┌──────────────────────┐
                                         │ oxidean-api (Axum)  │
                                         │                      │
                                         │  /v2/*  OCI module   │
                                         │  /npm/* npm module   │
                                         │  /generic/* raw      │
                                         │  /api/rpc package.*  │
                                         │  token realm (OCI)   │
                                         └──────────┬───────────┘
                                                    │
                          ┌─────────────────────────┼─────────────────────────┐
                          ▼                         ▼                         ▼
                 package ACL helper          content-addressed           oxidean-db
                 (Capability + PAT           blob store                  packages /
                  package:read/write)        OXIDEAN_PACKAGES_DIR       blobs / refs
                          │                         │                         │
                          └────────────┬────────────┴────────────┬────────────┘
                                       ▼                         ▼
                              owner/repo Octane UI         Admin quota UI
                              list / delete confirm        usage breakdown
```

### Recommended Project Structure
```
crates/oxidean-api/src/
├── packages/
│   ├── mod.rs              # shared types, mount helpers
│   ├── acl.rs              # package Capability + visibility
│   ├── auth.rs             # PAT Basic + OCI Bearer token realm
│   ├── store.rs            # content-addressed FS + refcount ops
│   ├── oci.rs              # /v2 Distribution Spec handlers
│   ├── npm.rs              # /npm/{owner}/… registry handlers
│   └── generic.rs          # /generic/{owner}/… handlers
├── routes/                 # wire mounts in router (alongside git_smart_http)
└── … existing pat/, repo/acl.rs (extend, do not fork)

crates/oxidean-db/migrations/{postgres,mysql,sqlite}/
└── 00xx_packages.sql       # packages, versions, blobs, blob_refs, quotas

apps/web/src/routes/
├── $owner.packages*.tsrx           # owner packages list
├── $owner.$repo.packages*.tsrx     # repo-linked packages
└── admin/…packages*                # quota/usage
```

### Pattern 1: OCI repository name = `owner/image` under `/v2`
**What:** Repository `<name>` in Distribution Spec is `/{owner}/{image}` (may include extra path segments for nested names). Client image ref: `{public_host}/{owner}/{image}:{tag}`.
**When to use:** All OCI push/pull (PKG-01). Matches Gitea naming `{registry}/{owner}/{image}`. [CITED: https://docs.gitea.com/usage/packages/container]
**Example:**
```text
# Client
docker login localhost -u USER -p oxidean_pat_…
docker tag alpine:latest localhost/acme/api:1.0.0
docker push localhost/acme/api:1.0.0

# Server paths (Distribution Spec end-1…end-10)
GET  /v2/
GET  /v2/acme/api/manifests/1.0.0
GET  /v2/acme/api/blobs/sha256:…
PUT  /v2/acme/api/manifests/1.0.0
GET  /v2/acme/api/tags/list
DELETE /v2/acme/api/manifests/sha256:…
```
[CITED: https://raw.githubusercontent.com/opencontainers/distribution-spec/v1.1.1/spec.md]

### Pattern 2: npm per-owner registry root under `/npm`
**What:** Treat `https://{host}/npm/{owner}/` as the npm registry URL. Packuments live at `/{package}` relative to that root; scoped packages use `/@scope%2fname` encoding as usual.
**When to use:** PKG-02; satisfies D-PKG-01 `/npm` prefix and D-PKG-03 standard client config.
**Example:**
```bash
npm config set @acme:registry https://localhost/npm/acme/
npm config set -- '//localhost/npm/acme/:_authToken' "oxidean_pat_…"
npm publish --access restricted
```
[CITED: https://docs.gitea.com/usage/packages/npm] (Gitea uses `/api/packages/{owner}/npm/` — Oxidean substitutes locked `/npm/{owner}/`)

### Pattern 3: Generic immutable file versions
**What:** `PUT/GET/DELETE /generic/{owner}/{name}/{version}/{filename}` with `409` on overwrite; version DELETE removes all files for that version id.
**When to use:** PKG-03 / D-PKG-16.
**Example:**
```bash
curl --user user:oxidean_pat_… --upload-file app.bin \
  https://localhost/generic/acme/tool/1.0.0/app.bin
```
[CITED: https://docs.gitea.com/usage/packages/generic]

### Pattern 4: Hybrid package auth
**What:** Registry clients authenticate with PAT (Basic and/or Bearer). Access = ACL Capability ∩ PAT package scopes. Web UI uses session RPC only.
**When to use:** All non-anonymous registry ops (D-PKG-04…06).
**Matrix (planner lock-in):**

| Action | Package visibility | ACL need | PAT need |
|--------|-------------------|----------|----------|
| Pull | public | — | none (anonymous OK) |
| Pull | private | Read+ | `package:read` (or write) |
| Publish / npm dist-tag / deprecate | any | Write+ | `package:write` |
| Delete version | any | Admin | `package:write` (+ classic delete if split — see discretion) |

Extend existing enums:

```rust
// Current classic / FG surface [VERIFIED: crates/oxidean-core/src/pat_types.rs:49-51]
// pub enum ClassicPatScope { Repo, }
// Add: PackageRead, PackageWrite  (serialized as "package:read" / "package:write")
// FG: add packages: ContentsPerm-like PackagesPerm { Read, Write }
```

Capability ladder already:

```24:28:crates/oxidean-api/src/repo/acl.rs
pub enum Capability {
    Read = 1,
    Write = 2,
    Admin = 3,
}
```
[VERIFIED: `crates/oxidean-api/src/repo/acl.rs:24-28`]

### Pattern 5: Content-addressed package blobs (LFS twin, not LFS store)
**What:** Store bytes at `{OXIDEAN_PACKAGES_DIR}/{algo}/{aa}/{bb}/{digest}`; DB rows track package→blob refs and refcounts; GC deletes refcount=0 after grace period.
**When to use:** All formats (OCI layers/configs/manifests bytes, npm tarballs, generic files) — D-PKG-07/08; mirror Phase 14 LFS decisions without sharing `OXIDEAN_LFS_DIR`.

### Anti-Patterns to Avoid
- **Serving registry routes from the web SPA:** Traefik priority-1 Host rule would HTML the `/v2` probe — break docker. Add API routers.
- **Session cookies for docker/npm:** Same pitfall as Smart HTTP (Phase 8 D-12). Ignore Cookie on registry paths.
- **Sharing LFS or release-asset volumes:** Violates D-PKG-07 / Phase 15 D-REL-04 separation.
- **Overwriting npm/generic versions:** Violates D-PKG-10; return conflict (npm EPUBLISHCONFLICT / HTTP 409).
- **Implementing Referrers/cosign as Phase 20 blocker:** Not required for docker/podman push/pull; clients fall back to referrers tag schema. [CITED: OCI Distribution Spec v1.1.1 “Unavailable Referrers API”]
- **Hand-editing `packages/api-client`:** Use `make rpc-gen` for package list/delete RPCs.
- **Dialect SQL in `oxidean-api`:** Package tables go through `oxidean-db` only.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| HTTP server / routing | Custom socket server | Existing Axum 0.8 | Already production path |
| SHA-256 digests | Custom hash | `sha2` crate | Edge cases + OCI digest format |
| Upload session IDs | Incrementing counters | `uuid` v4 | OCI upload Location UUIDs |
| ACL capability ladder | Parallel permission enums | Extend `repo/acl.rs` patterns | Avoid web/git/package drift |
| PAT mint/hash/reveal UX | New token system | Extend `pat_types` + `/settings/tokens` | Users already have classic/FG flows |
| Type-to-confirm delete | Soft confirm only | Phase 15 transfer confirm pattern | D-PKG-12 |
| Full CNCF Distribution binary | Sidecar registry process | In-API handlers | One product, one Compose service |

**Key insight:** Protocol surface is large, but storage/auth/ACL should be **one shared package subsystem** — three thin protocol adapters, not three products.

## Common Pitfalls

### Pitfall 1: Traefik / Vite miss registry prefixes
**What goes wrong:** `GET /v2/` returns SPA HTML → docker “unsupported protocol” / misleading 404.
**Why it happens:** Compose today only routes `/api`, `/uploads`, `/health`, and `.git` to API. [VERIFIED: `docker-compose.yml:83-89`]
**How to avoid:** Add Traefik routers `PathPrefix(/v2|/npm|/generic)` priority ≥110 → api; Vite `proxy` entries for local `make dev`.
**Warning signs:** Browser shows app shell for `/v2/`; smoke `curl -I /v2/` not JSON/401 from API.

### Pitfall 2: Username collision with `v2` / `npm` / `generic`
**What goes wrong:** Owner slug steals registry path or SPA route.
**Why it happens:** Flat `/{owner}/{repo}` + reserved list currently omits these. [VERIFIED: `crates/oxidean-core/src/auth_types.rs:254-302`] — list ends at `"oauth2"`; no `v2`/`npm`/`generic`.
**How to avoid:** Add `v2`, `npm`, `generic` to `RESERVED_USERNAMES` in the same phase as routes.
**Warning signs:** Org create succeeds for slug `npm`; registry 404s ambiguously.

### Pitfall 3: Docker auth challenge mismatch
**What goes wrong:** `docker login`/`push` loops 401 or never sends PAT correctly.
**Why it happens:** Clients expect `WWW-Authenticate: Bearer realm=…,service=…,scope=repository:…:pull,push` then token GET. [CITED: https://distribution.github.io/distribution/spec/auth/token/]
**How to avoid:** Same-host token endpoint (recommend `/v2/token` or `/auth/token` under API); Basic to realm with PAT-as-password; mint short-lived opaque Bearer; scope intersected with ACL∩PAT.
**Warning signs:** Works with `curl -u` but not `docker push`.

### Pitfall 4: npm tarball URLs point at wrong host/path
**What goes wrong:** Install resolves packument then 404s tarball.
**Why it happens:** Packument `dist.tarball` must be absolute URL under `/npm/{owner}/…/-/….tgz` using `OXIDEAN_PUBLIC_ORIGIN`, not request Host alone (SSRFish Host injection / Compose internal hostnames).
**How to avoid:** Build tarball URLs from `OXIDEAN_PUBLIC_ORIGIN` (same as clone URLs Phase 8 D-19).
**Warning signs:** Publish OK, `npm install` fails fetching tarball.

### Pitfall 5: Blob GC deletes live layers
**What goes wrong:** Cross-package OCI dedup GC removes a still-referenced layer.
**Why it happens:** Refcount bugs on tag retarget / manifest delete / failed upload abort.
**How to avoid:** Refcount only on committed manifests/versions; incomplete uploads never increment live refs; GC grace window + dry-run metrics in Admin.
**Warning signs:** Pull after unrelated delete returns `BLOB_UNKNOWN`.

### Pitfall 6: Treating public npm “never reuse version” as D-PKG-10
**What goes wrong:** Over-constrain delete semantics.
**Why it happens:** Public npm forbids reusing `name@version` even after unpublish. [CITED: https://docs.npmjs.com/policies/unpublish/]
**How to avoid:** Follow locked D-PKG-10: **no overwrite while present**; after delete, republish of same version is allowed (GitHub Packages–style operational delete), unless discuss re-locks tombstones.
**Warning signs:** Users cannot recover from mistaken publish without forever bumping versions after delete.

### Pitfall 7: Linked-repo visibility vs independent package visibility
**What goes wrong:** Private repo’s linked package anonymously pullable (or reverse).
**Why it happens:** GitHub supports granular package permissions separate from repo. [CITED: https://docs.github.com/en/packages/learn-github-packages/about-permissions-for-github-packages]
**How to avoid (discretion recommendation):** Linked → inherit repo visibility + ACL; unlinked → package-level `public|private` flag defaulting private; document clearly in UI.

## Code Examples

### OCI endpoint subset to implement (v1.1.1)
```text
# Source: https://raw.githubusercontent.com/opencontainers/distribution-spec/v1.1.1/spec.md (Endpoints table)
end-1   GET            /v2/
end-2   GET|HEAD       /v2/<name>/blobs/<digest>
end-3   GET|HEAD       /v2/<name>/manifests/<reference>
end-4a  POST           /v2/<name>/blobs/uploads/
end-4b  POST           /v2/<name>/blobs/uploads/?digest=<digest>
end-5   PATCH          /v2/<name>/blobs/uploads/<uuid>
end-6   PUT            /v2/<name>/blobs/uploads/<uuid>?digest=<digest>
end-7   PUT            /v2/<name>/manifests/<reference>
end-8a  GET            /v2/<name>/tags/list
end-8b  GET            /v2/<name>/tags/list?n=<n>&last=<tag>
end-9   DELETE         /v2/<name>/manifests/<reference>
end-10  DELETE         /v2/<name>/blobs/<digest>
end-11  POST           mount (optional — implement if cheap; else 202 fallback)
end-12  GET referrers  — DEFER (return 404; clients use tag schema)
```

### npm API subset (D-PKG-14)
```text
# Source: https://raw.githubusercontent.com/npm/registry/main/docs/REGISTRY-API.md
GET  /npm/{owner}/{package}                 # packument
GET  /npm/{owner}/{package}/{version}       # version manifest
GET  /npm/{owner}/{package}/-/{tarball}.tgz # tarball (via dist.tarball)
PUT  /npm/{owner}/{package}                 # publish (+ attachments) / deprecate fields
GET|PUT|DELETE /npm/{owner}/-/package/{package}/dist-tags[/{tag}]
GET  /npm/{owner}/-/v1/search?text=&size=&from=
DELETE … unpublish paths (version delete)   # Admin ACL; type-to-confirm in UI
```

### PAT scope extension sketch
```rust
// Source: extend crates/oxidean-core/src/pat_types.rs (current Repo-only classic)
// [VERIFIED baseline]: ClassicPatScope::Repo only today (pat_types.rs:49-51)

#[serde(rename_all = "lowercase")]
pub enum ClassicPatScope {
    Repo,
    #[serde(rename = "package:read")]
    PackageRead,
    #[serde(rename = "package:write")]
    PackageWrite,
}
```

### Traefik label additions (Compose)
```yaml
# Mirror api-git priority pattern [VERIFIED: docker-compose.yml:83-86]
- traefik.http.routers.api-packages.rule=Host(`localhost`) && (PathPrefix(`/v2`) || PathPrefix(`/npm`) || PathPrefix(`/generic`))
- traefik.http.routers.api-packages.priority=110
- traefik.http.routers.api-packages.service=api
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Docker Registry HTTP API V2 (Docker-only) | OCI Distribution Spec | OCI standardization | Spec is content-agnostic; Image types via Image Spec |
| Distribution Spec without referrers | Spec v1.1.x + optional Referrers API | ~OCI 1.1 | Cosign/attestations nicer, but optional with tag-schema fallback |
| CouchDB-shaped npm registry | Still packument + attachments; search via `/-/v1/search` | ongoing | Self-host must speak classic publish PUT, not only “modern” REST |
| Host-only registries | Forge same-host path prefixes (`/v2`, per-owner npm) | Gitea/GHCR patterns | Matches D-PKG-01; requires edge routing care |

**Deprecated/outdated:**
- Docker schema1 manifests: optional legacy; do not require. [CITED: OCI Distribution Spec “Legacy Docker support”]
- Assuming cosign needs native Referrers: false for Phase 20 scope — clients fall back. [CITED: OCI Spec Unavailable Referrers API]

## Discretion Recommendations (for planner)

| Topic | Recommendation | Confidence |
|-------|----------------|------------|
| OCI spec pin | **v1.1.1**; implement end-1…10 (+ end-11 best-effort); **defer end-12 referrers** | HIGH |
| Cosign/provenance | **Defer** — not required for docker/podman/buildah push/pull | HIGH |
| npm registry URL | **`/npm/{owner}/`** as registry root | HIGH |
| Generic paths | **`/generic/{owner}/{name}/{version}/{file}`**; Basic auth; `Authorization: token <pat>` optional alias | MEDIUM |
| Visibility | Linked package **inherits** repo visibility/ACL; unlinked has **independent** `public\|private` (default private) | MEDIUM |
| Quotas | Defaults: max blob 2 GiB; per-owner 10 GiB; Admin override; breakdown by format + package | LOW (numbers) |
| GC | Periodic job every 24h; grace 7d after refcount 0; never GC open upload sessions | MEDIUM |
| Delete PAT | `package:write` + ACL Admin sufficient (no separate `package:delete` unless UX needs classic parity with GitHub `delete:packages`) | MEDIUM |
| After-delete republish | **Allow** same npm/generic version after delete (D-PKG-10 “delete to remove”) | HIGH (locked reading) |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Next DB migration number continues after `0010_orgs_acl` (Phases 11–19 may claim numbers first) | Standard Stack | Wrong migration id / merge conflict — planner must allocate at execute time |
| A2 | Default quota numbers (2 GiB / 10 GiB) | Discretion | Too tight/loose for operators |
| A3 | `/v2/token` as Bearer realm path is acceptable to docker/podman | Pitfalls / Auth | Client incompatibility — may need `/token` or `/jwt/auth` |
| A4 | After delete, republish same npm/generic version allowed | Pitfall 6 | If user wanted public-npm tombstones, security/stability story changes |
| A5 | `uuid`/`tempfile` legitimacy OK without re-running seam (already in tree) | Package Audit | Negligible |

**If this table is empty:** All claims verified — N/A (assumptions remain above).

## Open Questions (RESOLVED)

1. **Migration numbering collision with Phases 11–19**
   - What we know: Postgres migrations currently end at `0010_orgs_acl`. [VERIFIED: listing `crates/oxidean-db/migrations/postgres/`]
   - What's unclear: Which phase executes first and claims `0011+`.
   - Recommendation: Planner uses placeholder `00xx_packages` and resolves at execute against latest migration.
   - RESOLVED: Plans 00/02 use `00xx_packages` placeholder; execute resolves the real id against the latest dialect migration at apply time.

2. **Classic PAT: does `repo` scope imply package access?**
   - What we know: Hybrid auth adds explicit `package:read/write` (D-PKG-04).
   - What's unclear: Whether classic `repo` alone grants packages (GitHub classic often needs `read:packages` separately).
   - Recommendation: **Do not** imply packages from `repo`; require package scopes (fail closed). Document migration tip in tokens UI.
   - RESOLVED: Fail closed — `repo` does not imply package access; require `package:read` / `package:write` (D-PKG-04; plan 03 auth + plan 11 tokens UI tip).

3. **OCI nested image names (`owner/a/b`)**
   - What we know: Spec `<name>` allows multi-segment paths; Gitea allows `owner/my/image`.
   - What's unclear: How optional repo-link maps for nested names.
   - Recommendation: Allow nested OCI names; repo link is metadata on package row, not path-encoded.
   - RESOLVED: Nested OCI names allowed; optional `repository_id` is package-row metadata, not encoded in the path (plans 04/05).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust / cargo | API handlers + tests | ✓ | rustc 1.100.0-nightly | — |
| Node / Bun | Vitest / web | ✓ | node v24.5.0 / bun 1.4.0 | — |
| npm CLI | PKG-02 smoke | ✓ | (volta npm present) | — |
| Docker engine | OCI client smoke | ✗ (not detected in this env) | — | Protocol tests via `curl`/hyper; smoke script skips without Docker (git HTTPS pattern) |
| Podman | Alternate OCI client | ✗ | — | Same as Docker |
| Traefik (Compose) | Path routing | via Compose image | v3.3 | Document reverse-proxy rules |

**Missing dependencies with no fallback:** none for planning/implementation of handlers.

**Missing dependencies with fallback:** Docker/Podman for live OCI smoke — use HTTP-level conformance tests + skip-if-missing smoke scripts (mirror `smoke-git-https`).

Step 2.6: External tools identified and probed as above.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo-nextest (Rust) + Vitest `^5` (web) |
| Config file | `.config/nextest.toml`; `apps/web/vitest.config.ts` |
| Quick run command | `cargo nextest run -p oxidean-api -- packages` (filter TBD) / `cd apps/web && bun run test:unit` |
| Full suite command | `make test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PKG-01 | Anonymous pull public OCI; auth push; tag list/delete | integration | `cargo nextest run -p oxidean-api -- oci_registry` | ❌ Wave 0 |
| PKG-02 | Publish packument+tarball; install metadata; dist-tag; search; deprecate | integration | `cargo nextest run -p oxidean-api -- npm_registry` | ❌ Wave 0 |
| PKG-03 | Generic PUT/GET/DELETE; 409 overwrite | integration | `cargo nextest run -p oxidean-api -- generic_registry` | ❌ Wave 0 |
| PKG-04 | Private deny anonymous; Write+/package scopes enforce | integration | `cargo nextest run -p oxidean-api -- package_acl` | ❌ Wave 0 |
| PKG-05 | List RPC; delete Admin+confirm | integration + web | `cargo nextest run … package_rpc`; Vitest owner packages route | ❌ Wave 0 |
| Edge | Traefik `/v2` → API | smoke | `scripts/smoke-packages.sh` (skip without Docker) | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** targeted nextest filter for touched protocol
- **Per wave merge:** `cargo nextest run -p oxidean-api -p oxidean-db` + web unit/integration for package routes
- **Phase gate:** `make test` + smoke-packages (skip-ok) green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/oxidean-api/tests/oci_registry.rs` — covers PKG-01
- [ ] `crates/oxidean-api/tests/npm_registry.rs` — covers PKG-02 / D-PKG-14
- [ ] `crates/oxidean-api/tests/generic_registry.rs` — covers PKG-03
- [ ] `crates/oxidean-api/tests/package_acl.rs` — covers PKG-04
- [ ] `crates/oxidean-db/tests/dialect_packages.rs` — multi-dialect metadata
- [ ] `apps/web/src/routes/…packages*.integration.test.ts` — PKG-05 UI
- [ ] `scripts/smoke-packages.sh` + `make smoke-packages` — edge routing
- [ ] Compose labels + `OXIDEAN_PACKAGES_DIR` volume + Vite proxies — ops Wave 0/early plan

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | PAT Basic / Bearer for registry; session cookies ignored on `/v2|/npm|/generic` |
| V3 Session Management | no (registry) / yes (UI) | UI stays opaque session cookies; registry never uses them |
| V4 Access Control | yes | `Capability` ACL + PAT `package:read/write` intersection; anti-enumeration for private |
| V5 Input Validation | yes | Digest/tag/name regexes from OCI Spec; npm package name rules; generic charset allowlist |
| V6 Cryptography | yes | SHA-256 content addressing via `sha2`; PAT hashes at rest (existing) — never hand-roll |

### Known Threat Patterns for forge registries

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Pull private package anonymously | Information Disclosure | D-PKG-05; 401/404 policy consistent with git vs web (prefer 401 for registry clients) |
| PAT with only `repo` scope publishes packages | Elevation of Privilege | Require explicit `package:write` (fail closed) |
| Overwrite immutable version (supply-chain) | Tampering | Reject overwrite; immutable digests; mutable tags only for OCI |
| Host header tarball SSRF / wrong origin | Spoofing | `OXIDEAN_PUBLIC_ORIGIN` for absolute URLs |
| Blob store path traversal via crafted digest/name | Tampering | Strict digest regex; reject `..`; store only hex shards |
| Quota bypass via parallel uploads | Denial of Service | Pre-declare size / enforce on stream; reject over max; quota check before commit |
| Cross-package blob GC ORphan abuse | Denial of Service | Refcounts + grace; rate-limit failed auth (reuse Smart HTTP limiter) |
| Cookie session accepted as registry auth | Elevation of Privilege | Ignore Cookie on registry routes (Phase 8 lesson) |

## Sources

### Primary (HIGH confidence)
- OCI Distribution Spec v1.1.1 — https://raw.githubusercontent.com/opencontainers/distribution-spec/v1.1.1/spec.md — endpoints, Pull MUST, referrers optional
- CNCF Distribution Token Auth — https://distribution.github.io/distribution/spec/auth/token/ — Bearer challenge flow
- npm Registry API — https://raw.githubusercontent.com/npm/registry/main/docs/REGISTRY-API.md — packument/search
- In-repo: `crates/oxidean-core/src/pat_types.rs`, `crates/oxidean-api/src/repo/acl.rs`, `docker-compose.yml`, `crates/oxidean-core/src/auth_types.rs` RESERVED_USERNAMES
- Phase contexts: 20/08/10/14/15 CONTEXT.md (locked decisions)

### Secondary (MEDIUM confidence)
- Gitea Container / npm / Generic docs — https://docs.gitea.com/usage/packages/container · `/npm` · `/generic` — forge client UX patterns
- GitHub Packages permissions — https://docs.github.com/en/packages/learn-github-packages/about-permissions-for-github-packages — granular vs inherited visibility
- npm unpublish policy — https://docs.npmjs.com/policies/unpublish/ — contrast with D-PKG-10 delete semantics

### Tertiary (LOW confidence)
- Default quota numeric recommendations (operator taste)
- Exact token realm path string preferred by all clients

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — reuse verified in-tree crates; no new installs
- Architecture: HIGH — locked path/auth/storage decisions + official OCI/npm/Gitea docs
- Pitfalls: HIGH — Traefik/Vite/reserved-name/auth-challenge grounded in repo + specs

**Research date:** 2026-09-14
**Valid until:** 2026-10-14 (30 days; pin OCI spec tag if planning slips)

## RESEARCH COMPLETE
