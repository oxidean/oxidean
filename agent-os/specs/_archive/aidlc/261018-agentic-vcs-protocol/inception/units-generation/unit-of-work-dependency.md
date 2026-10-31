# Unit Dependency DAG — Agentic VCS Protocol

Topology only — this artifact declares what can depend on what. Sequencing (what ships first, Bolt order, critical path) is Delivery Planning's decision (2.9).

```yaml
units:
  - name: u1-vcs-wire-foundation
    kind: library
    depends_on: []
  - name: u2-vcs-persistence
    kind: library
    depends_on: [u1-vcs-wire-foundation]
  - name: u3-vcs-object-store
    kind: library
    depends_on: [u2-vcs-persistence]
  - name: u4-vcs-changeset-core
    kind: library
    depends_on: [u3-vcs-object-store]
  - name: u5-vcs-merge-engine
    kind: library
    depends_on: [u4-vcs-changeset-core]
  - name: u6-vcs-git-interop
    kind: library
    depends_on: [u4-vcs-changeset-core]
  - name: u7-vcs-api-surface
    kind: library
    depends_on: [u2-vcs-persistence, u3-vcs-object-store, u4-vcs-changeset-core, u5-vcs-merge-engine, u6-vcs-git-interop]
  - name: u8-vcs-cli
    kind: library
    depends_on: [u7-vcs-api-surface]
  - name: u9-vcs-mcp
    kind: library
    depends_on: [u7-vcs-api-surface]
  - name: u10-vcs-web-surfaces
    kind: ui
    depends_on: [u7-vcs-api-surface]
```

## Why these edges

- `u1 → u2`: the façade's activity-insertion path consumes the shared `vcs.*` kind constants from `oxidean-core`; row types otherwise stand alone.
- `u2 → u3`: `ObjectEntry`/`ObjectLink` rows + quota attribution need the façade.
- `u3 → u4`: the create tx verifies blob existence and inserts `ObjectLink` rows — ObjectStore must exist first.
- `u4 → u5`, `u4 → u6`: merge reads index+blobs and writes through ChangesetStore; interop creates changesets and reads chains — both need the core write path.
- `u2,u3,u4,u5,u6 → u7`: the API surface's direct edges — VcsApi consumes ChangesetStore/RefRegistry/ReadIndex (`u4`), hosts the object PUT/GET routes (`u3`), reads `RepoSurfaceFlag` and calls `insert_repo_activity` through the façade (`u2`), and dispatches merge/interop (`u5`/`u6`); it also supplies `ProtectionChecker`/`GitBackend` injection into `u6`.
- `u7 → u8,u9,u10`: all three client surfaces call live `vcs.*` endpoints / consume the generated client — nothing to build against before the procedures exist.


## Parallel development opportunities

- `{u5-vcs-merge-engine, u6-vcs-git-interop}` — independent once U4 lands; both only consume the core write path.
- `{u8-vcs-cli, u9-vcs-mcp, u10-vcs-web-surfaces}` — three independent client surfaces once U7 lands.
- Multiple valid topological orderings exist around the U4 bottleneck (e.g. U5 before U6, or interleaved).

## Integration points

| Between | Contract |
|---|---|
| U1 → all | `oxidean-core` wire DTOs + `proc_classes` + `vcs.*` activity-kind constants; generated `@oxidean/api-client` |
| U2 → U3,U4,U6,U7 | `VcsPersistence` façade signatures (entity rows, CAS write, index rows, `insert_repo_activity`) |
| U3 → U4 | `ObjectLink` insert inside the changeset create tx; blob-hash verification |
| U4 → U5 | merge/rebase write path = ChangesetStore create; `merge_base` via ReadIndex |
| U4,U5,U6 → U7 | component interfaces consumed by `VcsApi`; `ProtectionChecker` trait + `Arc<dyn GitBackend>` injected by `oxidean-api` |
| U7 → U8,U9,U10 | live `vcs.*` procedures + `/api/vcs/…/objects` routes; generated client types |
| U7 host wiring | `OXIDEAN_VCS_*` env vars documented/wired (`.env.example`, `docs/CONFIGURATION.md`, compose/`./var`, `docs/DEPLOYMENT.md`, `.railway/railway.ts`); factory-reset sweep; GC job spawn — U3 owns the var contract, U7 owns the host wiring |
| U8 → U7 (runtime) | `WorkingCopy` want-fetch GETs + `vcs.changeset_get` RPC |

## Skeleton note

This intent's scope resolves `skeleton: off` (practices-discovery affirmed slice-first sequencing without a walking-skeleton gate). The DAG is still front-loadable: U1+U2+U3+U4 form the smallest runnable write path (flag on → PUT object → create changeset → read it back) if Delivery Planning wants an early integrated check.
