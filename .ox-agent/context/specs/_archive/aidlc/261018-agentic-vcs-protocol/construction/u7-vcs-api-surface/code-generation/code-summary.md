# Code Summary — u7-vcs-api-surface

## What landed

The full 17-procedure `vcs.*` RPC surface plus the two object routes, wired end-to-end through the host. Upstream design stages were workflow-skipped for u7–u10 under the floor-16 jump, so the plan embedded the design decisions sourced from the frozen contract; the two contract corrections found during implementation are folded in (see below).

### `oxidean-vcs` (domain orchestration)

- **`src/api.rs`** (~1,100 prod + ~1,250 test lines) — `VcsApi`: the three-case flag gate (`require_flag_row` for reads — a disabled row still serves, an absent row denies all incl. `status`; `require_enabled` for writes), all 17 procedures, the lazy-import retry-once on git-oid read misses (`resolve_git_oid`/`lazy` — `.boxed()` to cap rustc's Send-unroll depth), default-head semantics (`main` → else sole head → none `vcs.not_found` → multi-head-no-main `vcs.invalid_input`), 500-entry merge-conflict cap, and a total typed error bridge (`map_changeset_err`/`map_index_err`/`map_ref_err`/`map_merge_err`/`map_store_err`/`map_interop_err` → C1 codes; skipped-db facades fail `db.not_configured`, opaque strings → `rpc.internal`, no raw detail on the wire).
- **`src/activity.rs`** — `ActivityRecorder`: exactly one `insert_repo_activity` row per successful mutation with the pinned C4 field mappings; zero rows on failure; empty actor/skipped db degrade to no-op.
- **`src/lib.rs`** — `pub mod api; pub mod activity;` under `#[cfg(feature = "server")]`.
- **`src/git_interop.rs`** — `tracing` `.entered()` guards across `.await` were the `!Send` root that broke axum's WS handler future; reworked `import_chain`/`lazy_import`/`export` to `.instrument(span).await` (no guard crosses a suspend point).

### `oxidean-api` (host wiring)

- **`src/vcs.rs`** (new) — 17 handlers: `{owner,name}` → `resolve_repo_for_read`/`resolve_repo_for_admin`/owner-mutate per the class table, `ensure_not_archived` on every write (reads still serve archived), `ctx.vcs` → `RpcResponse` envelope; `VcsSlot { api, objects_dir, gc_interval_secs }` + `SharedVcs`.
- **`src/vcs_routes.rs`** (new) — `GET /api/vcs/{owner}/{name}/objects/{hash}` (octet-stream + immutable cache; flag-ROW gate — `enabled:false` still serves) and `POST …/objects` (enabled-only flag gate; streams into `ObjectStore::put`; `{ok:true,data:{hash}}`, dedup no-op). Bearer/PAT only — cookie-only ⇒ `401 vcs.unauthenticated`; `vcs.object_get`/`vcs.object_put` aliases feed PAT classification but are never in `PROCEDURES`. `DefaultBodyLimit` 512 MiB sits above `max_object_bytes` (256 MiB) so typed `vcs.quota_exceeded` beats untyped 413.
- **`src/vcs_protection.rs`** (new) — `HostProtectionChecker` → `check_ref_update`, `Denied`→`vcs.forbidden{push_hook}`, `Conflict`→`vcs.ref_conflict{actual}`.
- **`src/rpc.rs`** — 17 real arms (`vcs::<op>(ctx, req.input).boxed()` — per-arm boxing enforces `Send` and shrinks `dispatch`'s coroutine witness past `recursion_depth_exceeding_limit`); the u1 stub-sentinel test inverted to `vcs_dispatch_arms_call_real_handlers` (every arm must call a real handler; `procedures_list_matches_dispatch` still green).
- **`src/app.rs`** — `AppState.vcs: Option<SharedVcs>` via `build_vcs_slot` (`OXIDEAN_VCS_*` → `ObjectStoreConfig::from_env`; malformed env / refused dir / caps rejection ⇒ `None` + `warn!` — never takes the instance down); `RpcCtx.vcs` plumbed to all ctx builders (+ two hand-built ctx literals in `repo_raw.rs`/`template_packs.rs`/`verify_reset.rs` tests); route mount + 512 MiB body limit.
- **`src/main.rs`** — `spawn_background_jobs` gains the slot; `#[tokio::main]` → explicit runtime `Builder` with `thread_stack_size(8 MiB)` — the ~25-layer debug coroutine poll chain (router→dispatch→handler→`VcsApi`→store→db→sqlx) overflows tokio's 2 MiB default in dev builds.
- **`src/jobs/schedule.rs`** — VCS object GC task: `Some(slot) && gc_interval_secs>0` ⇒ interval loop calling `objects().gc_once()`; counts+bytes-only logs.
- **`src/auth/admin.rs`** — factory-reset `database_and_repositories` sweeps `slot.objects_dir` (skipped when `vcs: None` — a refused/foreign dir is not ours to wipe).
- **`src/repo/activity.rs`** — `repo.activity.list` `push_type` filter accepts `VCS_ACTIVITY_KINDS` alongside `VALID_PUSH_TYPES`; git kinds untouched.
- **`src/lib.rs`** + **`Cargo.toml`** — module decls + `oxidean-vcs = { features = ["server"] }` dep.
- **`tests/vcs_api.rs`** (new) — 13 integration tests through the real router: flag-gate three-case table, repo_enable/status/activity rows, read/write tiers, ref CAS + conflict, archived write-deny/read-pass, object PUT/GET roundtrip + flag gates + cookie-deny + capability-deny, factory-reset sweep, foreign-dir boot degradation, GC job invocation, route-alias classification.

### Operator surface

`.env.example`, `docs/CONFIGURATION.md`, `docs/DEPLOYMENT.md`, `docker-compose.yml` (`OXIDEAN_VCS_OBJECTS_DIR` + `/var/vcs-objects` volume), `.railway/railway.ts` env passthrough — frozen defaults (objects `var/vcs-objects`, max object 256 MiB, repo quota 10 GiB, GC interval 3600 s, grace 86400 s).

### `.cargo/config.toml` (new)

`[env] RUST_MIN_STACK=16777216` — the same fat-debug-chain overflow hit libtest's 2 MiB test threads (vcs_api + pre-existing `actions_workflow_templates` both aborted). One env line fixes `cargo test`, `cargo nextest`, and `cargo run` dev without touching every test file.

## Contract corrections folded in during implementation

1. **Three-case flag gate** — the plan's two-case sketch was corrected to contract truth: absent row ⇒ deny ALL (incl. `status`); `enabled:false` ⇒ reads pass, writes deny; `enabled:true` ⇒ full. GET route = row-present gate; POST route = enabled gate.
2. **Default-head semantics** — no refs ⇒ `vcs.not_found` (blame/log); multiple heads without `main` ⇒ `vcs.invalid_input`.
3. **Stack-depth fixes** — `!Send` futures (tracing guards) → `.instrument()`; `recursion_depth_exceeding_limit` → per-arm `.boxed()` + `lazy(...).boxed()`; 2 MiB debug-stack overflow → prod 8 MiB workers + `RUST_MIN_STACK` for tests.
4. **`repo_enable` skipped-db gap** — flag-free path bypassed `flag_row`, mapping the skipped-db facade error to `rpc.internal`; explicit `is_skipped` → `db.not_configured` now precedes the upsert.

## Gate results

| Gate | Result |
|------|--------|
| `cargo check -p oxidean-api` / `--bins` | clean, zero warnings |
| `cargo fmt --all` | clean |
| `cargo clippy -p oxidean-api -p oxidean-vcs` | clean |
| `cargo test -p oxidean-vcs --features server` | **178/178** (30 `api` + 3 `activity` new) |
| `cargo test -p oxidean-api --test vcs_api` | **13/13** |
| `cargo test -p oxidean-api --lib` | **232/232** (incl. inverted dispatch sentinel) |
| `cargo test -p oxidean-api --test actions_workflow_templates` | 9/9 (pre-existing overflow fixed by `RUST_MIN_STACK`) |
