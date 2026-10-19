# Units of Work — Agentic VCS Protocol

Ten units, each a construction-deliverable slice aligned to the domain component boundaries (Q1 answer A). Every unit is `embedded` — no new binary, container, or service; `oxidean-vcs` ships as a workspace crate consumed by `oxidean-api` (server feature) and `oxidean-cli` (client feature), plus host-side deltas in `oxidean-core`, `oxidean-db`, and `apps/web`.

| Unit ID | Directory | Name | Kind | Complexity | Deployment |
|---------|-----------|------|------|------------|------------|
| U1 | `u1-vcs-wire-foundation` | VCS wire foundation | library | S | embedded (`oxidean-core` + generated client) |
| U2 | `u2-vcs-persistence` | VCS persistence façade | library | M | embedded (`oxidean-db`) |
| U3 | `u3-vcs-object-store` | Object store + GC | library | M | embedded (`oxidean-vcs`; env-var contract only — wiring in U7) |
| U4 | `u4-vcs-changeset-core` | Changeset/ref/index core | library | L | embedded (`oxidean-vcs`) |
| U5 | `u5-vcs-merge-engine` | Merge/rebase engine | library | M | embedded (`oxidean-vcs`) |
| U6 | `u6-vcs-git-interop` | Git import/export interop | library | L | embedded (`oxidean-vcs` + api injection) |
| U7 | `u7-vcs-api-surface` | RPC + routes + activity surface | library | L | embedded (`oxidean-vcs` + `oxidean-api` glue) |
| U8 | `u8-vcs-cli` | `ox vcs` CLI + working copy | library | M | embedded (`oxidean-vcs` client feature + `oxidean-cli` glue) |
| U9 | `u9-vcs-mcp` | MCP tool surface | library | S | embedded (`oxidean-vcs` + `oxidean-api` MCP wiring) |
| U10 | `u10-vcs-web-surfaces` | Web settings + activity feed | ui | M | embedded (`apps/web`) |

## U1 — vcs-wire-foundation

- **Owns:** all `vcs.*` request/response DTOs, typed error codes, `RpcRequest`-shaped wire types, `proc_classes` `vcs.*` entries (fail-closed `SessionOnly` for unclassified), shared `vcs.*` activity-kind constants — all in `oxidean-core` (ADR-002); `versioning.toml` component registration + scope mapping (FR15.4); `make rpc-gen` regeneration of `packages/api-client`.
- **Boundaries:** wire contract only — no logic, no SQL, no route code. Domain-internal types stay in `oxidean-vcs` and must not leak onto the wire.
- **Why first-shaped:** every other unit consumes its types; it is the only unit with no `depends_on`.
- **Notes:** `vcs.status` schema is provisional — contract design freezes it before the client template generates. Ref field name is `ref` (ADR-009).

## U2 — vcs-persistence

- **Owns:** the `oxidean-db::vcs` module — migrations ×3 (postgres/mysql/sqlite, filename parity) for RepoSurfaceFlag, ObjectEntry, ObjectLink, Changeset, FileChange, Ref, DerivedIndex, ImportLink; row types; façade methods (`Database`-by-value signature, `skipped()` → `db.not_configured`); repo-row cascade deletion; the `insert_repo_activity` generalization (nullable push columns or generalized signature — contract-design decision).
- **Boundaries:** sole dialect-SQL site; nothing outside `oxidean-db` writes SQL.
- **Notes:** depends on U1 only through the shared activity-kind constants. The push-shaped activity signature is the riskiest seam — resolve the nullability call in contract design before Bolting this.

## U3 — vcs-object-store

- **Owns:** `ObjectStore` — global `ObjectEntry` CAS rows + per-repo `ObjectLink` rows, temp→rename→row PUT ordering, hash verification, quota enforcement, streamed object I/O, grace-windowed GC with computed referenced-ness, row-less/`*.tmp` sweeps, boot-time stale-lock clear.
- **Operator surface:** owns the env-var contract — names, semantics, and defaults for `OXIDEAN_VCS_OBJECTS_DIR`, `OXIDEAN_VCS_GC_INTERVAL_SECS`, `OXIDEAN_VCS_GC_GRACE_SECS`, `OXIDEAN_VCS_MAX_OBJECT_BYTES`, `OXIDEAN_VCS_REPO_QUOTA_BYTES`. The host-side wiring of those vars (`.env.example` + `docs/CONFIGURATION.md` entries, compose env/`./var` bind, `docs/DEPLOYMENT.md` volume table, `.railway/railway.ts`) and the factory-reset sweep are U7's — they are `oxidean-api`/repo-level host changes.
- **Boundaries:** no auth inside; VcsApi gates first. GC job registration lands in U7's host wiring (job spawn lives in `oxidean-api`).
- **Notes:** GC is single-node-scale; horizontally scalable storage is the v2 shape, not this cut.

## U4 — vcs-changeset-core

- **Owns:** `ChangesetStore` (create/lookup/lineage, idempotent resubmit, canonical hash inputs incl. author identity, same-parent concurrency, empty-parent bootstrap, single create tx covering changeset + file changes + ObjectLink insert + index update), `RefRegistry` (minimal refs, CAS `ref_update`, create/delete/list), `ReadIndex` (incremental DerivedIndex maintenance via the create-tx hook — upsert/merge semantics for concurrent same-parent children; table-level reads of changeset rows via the façade, no back-edge into ChangesetStore).
- **Boundaries:** the WAL-equivalent write path; strictest invariants in the crate.
- **Notes:** `vcs.file_get` inline-size cap bound is contract-design owned. ImportLink DDL lives in U2 even though U6 owns the entity behaviourally.

## U5 — vcs-merge-engine

- **Owns:** `MergeEngine` — merge_base via the index, diff3 text merge under `spawn_blocking` with input caps (blob size + changed-file count), typed file-level conflicts `{path, kind}` for binary/mode/type/rename, rebase replay; writes result changesets through ChangesetStore's normal create path; never moves refs implicitly.
- **Boundaries:** pure-ish computation over blobs + index — separately testable.
- **Notes:** conflict payload beyond `{path,kind}` is contract-design owned.

## U6 — vcs-git-interop

- **Owns:** `GitInterop` — deterministic import (explicit `vcs.git_import` + the single-chain lazy-import entry point AC7.1.2), `ImportLink` rows, export conversion, and the `ProtectionChecker` trait port defined in-crate; `oxidean-api` injects the real protection impl + `Arc<dyn GitBackend>` at dispatch (ADR-006).
- **Boundaries:** branch moves go only through `fast_forward_ref`/`force_update_ref` (hook-driven); never bare `update_ref`. `--force` overrides fast-forward only.
- **Notes:** export pays an O(repo) temp clone per call — recorded v1 debt. Import commits-per-call cap + resumable cursor bound is contract-design owned.

## U7 — vcs-api-surface

- **Owns:** `VcsApi` orchestration (two-case flag gate: absent row denies all incl. `vcs.status`; `enabled:false` denies writes only; authz classes; archived-write `repo.archived`; typed taxonomy; lazy-import trigger on git-oid read misses), `ActivityRecorder` (op-granular rows via the façade, never per-object), and all host wiring: `PROCEDURES`/dispatch arms, `proc_classes` consumption, `/api/vcs/` route nest with explicit `DefaultBodyLimit` on the object PUT route, `AppState` VCS slot, `ProtectionChecker`/`GitBackend` injection, `ensure_not_archived` on write-class arms, `VALID_PUSH_TYPES` extension from shared constants, MCP Access gating, GC job spawn + stats logging, factory-reset sweep.
- **Boundaries:** cookie-auth requests never reach here (transport denies upstream); unlisted procedures default `SessionOnly`. U7 also owns the host-side `OXIDEAN_VCS_*` wiring: `.env.example` + `docs/CONFIGURATION.md` + compose env/`./var` bind + `docs/DEPLOYMENT.md` + `.railway/railway.ts`, and the factory-reset sweep.
- **Notes:** activity op→row mapping depends on U2's signature decision — sequence it after contract design resolves the nullability call.

## U8 — vcs-cli

- **Owns:** `CliSurface` (the 16-command `ox vcs` set — `show`/`cat` distinct), `WorkingCopy` (sparse manifest filter, want-fetch, bounded hash-verified cache, `CheckoutResult` reporting), `oxidean-cli` argv glue (`Command` variants + USAGE rows + `required_procedures` entries).
- **Boundaries:** `oxidean-cli` depends on `oxidean-vcs` with the `client` feature only — no sqlx/`oxidean-git` in `ox` (ADR-011); local typed errors never masquerade as `vcs.*` codes.
- **Notes:** output discipline `Out::Rpc` + `ox: <code>: <message>`; `vcs.unauthenticated`/`vcs.forbidden` join exit-3 in `error_exit_code`'s match list.

## U9 — vcs-mcp

- **Owns:** `McpSurface` — descriptors for the 12-tool FR13.1 set with separate `owner`/`name` args; `oxidean-api::mcp` wraps with Access gating + `isError` mapping.
- **Boundaries:** descriptors only; deliberate absences (repo_enable, ref_update, git_import, git_export, checkout, and pending contract-design `changeset_get`) stay absent.
- **Notes:** whether `vcs.changeset_get` joins the tool set is contract-design owned.

## U10 — vcs-web-surfaces

- **Owns:** `VcsWebSurfaces` — the Octane `VcsToggle` in `[owner]/[repo]/settings` (unchecked-default on `vcs.disabled`, loading/saving/error/read-only states; the lacks-admin disabled prong is a dead state — the page gates on `can_admin` first), `repo-activity-row` `actionCopy`/`commitHref` handling of `vcs.*` kinds (no `cs_`-as-git-commit 404s), activity filter chips for `vcs.*` kinds.
- **Boundaries:** the only browser surfaces; all domain logic via `@oxidean/api-client` (TanStack Query/Form).
- **Notes:** `make web-lint` + `make web-format-check` gates apply.
