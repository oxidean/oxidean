# Phase 20: Packages Registry - Context

**Gathered:** 2026-09-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Instance hosts a multi-format package registry (OCI, npm, generic/raw) with the same auth/visibility rules as owning users/orgs/repos. Delivers PKG-01…PKG-05.

**Requirements:** PKG-01, PKG-02, PKG-03, PKG-04, PKG-05

**Success criteria (from ROADMAP):**
1. Publish/pull OCI images scoped to repo or org
2. Publish/pull npm packages scoped to repo or org
3. Publish/pull generic/raw packages scoped to repo or org
4. Packages respect owning repo/org auth and visibility
5. List and delete package versions the user may manage

**Out of scope:**
- Maven/NuGet/PyPI/other ecosystems (not in PKG-*)
- Signing / cosign / provenance attestations (unless needed for minimal OCI compliance — Claude discretion for must-have vs defer)
- Paid/storage billing meters
- Geo-replicated storage / S3 backends (filesystem volume only, like LFS)

**UI hint:** yes — owner packages pages, repo-linked package views, Admin quota/usage.

</domain>

<decisions>
## Implementation Decisions

### A — Scope & routing
- **D-PKG-01:** Registry on the **same host**, **path-based**: OCI `/v2/…`, npm `/npm/…`, generic `/generic/…` — **Reversibility:** costly — client config URLs
- **D-PKG-02:** Packages namespaced by **owner + name** (user or org); **optional link to a repository** — **Reversibility:** costly — ownership model
- **D-PKG-03:** Prefer **standard client URL layouts** for docker/npm/generic tools (not a single `/packages/{owner}`-only tree) — **Reversibility:** costly

### B — Auth
- **D-PKG-04:** **Hybrid auth:** derive access from **owner ACL + existing PAT scopes**, **and** add fine-grained **`package:read` / `package:write`** — **Reversibility:** costly — token UX + ACL matrix
- **D-PKG-05:** **Anonymous pull** for **public** packages; auth required for private pulls and **all** publishes — **Reversibility:** reversible
- **D-PKG-06:** **Write+** on owning repo/org may **publish**; **Admin** may **delete** — **Reversibility:** reversible

### C — Storage
- **D-PKG-07:** Separate **`OXIDEAN_PACKAGES_DIR`** volume (not LFS, not release-assets) — **Reversibility:** costly — ops/backup split
- **D-PKG-08:** **Content-addressed** blob store with **cross-package dedup** (especially OCI layers) — **Reversibility:** costly — GC/refcount
- **D-PKG-09:** **Max blob size + per-owner quotas** via env defaults + Admin UI; **reject** over-limit uploads — **Reversibility:** reversible

### D — Lifecycle / UI
- **D-PKG-10:** **GitHub-aligned immutability:** OCI **digests immutable**, **tags mutable**; npm and generic **versions immutable** (no overwrite; delete to remove). No separate yank state — **Reversibility:** costly — format semantics
- **D-PKG-11:** UI: **owner packages page** + **repo-linked** package views when linked — **Reversibility:** reversible
- **D-PKG-12:** Delete version requires **type-to-confirm** (`name@version` / equivalent) — **Reversibility:** reversible

### E — Format depth
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

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 20 goal, PKG-01…05
- `.planning/REQUIREMENTS.md` — PKG-01…05 wording
- `.planning/phases/08-git-https-pats/08-CONTEXT.md` — PAT model to extend with package scopes
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — owner ACL / visibility
- `.planning/phases/14-git-lfs/14-CONTEXT.md` — separate volume + quota/GC patterns (do not share store)
- `.planning/phases/15-releases-transfer/15-CONTEXT.md` — release-assets volume stays separate; transfer ownership implications

### Code / ops mirrors
- `crates/oxidean-api/src/routes/git_smart_http.rs` — PAT Basic patterns
- `crates/oxidean-api/src/repo/acl.rs` — capability checks to mirror for package owner
- `docker-compose.yml` — volume + Traefik path routing for `/v2`, `/npm`, `/generic`
- `docs/CONFIGURATION.md` / `docs/API.md` — document registry endpoints and env

### External specs (pin revisions while researching)
- OCI Distribution Spec
- npm registry API docs
- GitHub Packages behavior notes (immutability / GHCR tags)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- PAT auth + fine-grained scope machinery (extend with package:read/write)
- Owner ACL (user/org) and repo visibility
- Volume + Admin quota UI patterns from LFS (Phase 14 decisions)
- Type-to-confirm danger UX from transfer/delete flows (Phase 15)

### Integration Points
- New Axum route modules for OCI / npm / generic
- Owner profile + org pages → Packages tab/list
- Repo page → linked packages
- Admin → package storage quotas/usage
- Transfer/rename (Phase 15) must update package owner paths/metadata

</code_context>

<specifics>
## Specific Ideas

- User asked to **match GitHub** for immutability semantics
- User asked for **fully featured** across OCI, npm, and generic — not a thin vertical slice
- Auth is explicitly **hybrid** (reuse ACL/scopes **and** new package FG scopes)

</specifics>

<deferred>
## Deferred Ideas

- Additional ecosystems (Maven, PyPI, NuGet, …)
- S3/remote blob backends
- Sigstore/cosign unless research proves required for stated OCI clients
- Cross-instance package replication

</deferred>

---

*Phase: 20-packages-registry*
*Context gathered: 2026-09-14*
