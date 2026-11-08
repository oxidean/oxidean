# Code Generation Plan — u7-vcs-api-surface

Upstream design stages (functional-design / nfr-requirements / nfr-design / infrastructure-design) were workflow-skipped for u7–u10 under the floor-16 forward jump; this plan embeds the design decisions those stages would have produced, sourced from the frozen contract (`inception/contract-design/contract-summary.md` C1–C5/C7/C9 + the `OXIDEAN_VCS_*` table), `inception/units-generation/unit-of-work.md` §U7, and `inception/requirements-analysis/requirements.md`.

## Deliverables

1. [ ] **`crates/oxidean-vcs/src/api.rs`** — NEW `server`-gated `VcsApi` orchestration. Constructor `VcsApi::new(db: Database, objects: Arc<ObjectStore>, changesets: Arc<ChangesetStore>, index: Arc<ReadIndex>, refs: Arc<RefRegistry>, interop: Option<Arc<GitInterop>>)` — `interop` optional so the API surface works on repos without git interop configured. Two-case flag gate `flag(repo_id, write: bool)` (C1/AC1.2.1): absent row ⇒ `VcsErrorCode::Disabled` for EVERY procedure including `vcs.status` (never-enabled denies all); `enabled:false` ⇒ `Disabled` on write-class only (reads still serve); `enabled:true` ⇒ pass. Methods — one per C1 procedure, repo-row scoped, returning wire DTOs: `repo_enable(actor, enabled)` (upsert via `upsert_repo_surface_flag`, records `enabled_by`); `status` (flag state + `changeset.count_for_repo` + `object_link.count_for_repo`); `changeset_create` (delegates `ChangesetStore::create` — idempotent byte-identical resubmit, AC2.2.x); `changeset_get`; `log`/`diff`/`tree`/`blame`/`path_history`/`merge_base`/`file_get` (delegates `ReadIndex` queries, `at` resolution per C1 pinned semantics: omitted `at` ⇒ `main` tip, else sole head, none ⇒ `vcs.not_found`, ≥2 refs w/o `main` ⇒ `vcs.invalid_input`); `merge`/`rebase` (delegates `MergeEngine`; `vcs.conflict{conflicts:[{path,kind,detail?}]}` denial, never `ok:true`; `rebase` rejects descendant/cycle target `vcs.invalid_input` AC2.4.3); `ref_list`; `ref_update` (delegates `RefRegistry::cas` — `expected_old:null`=create-only, `new:null`=delete, NON-idempotent AC5.1.3: retried landed call ⇒ `vcs.ref_conflict{data:{actual}}`); `git_import`/`git_export` (delegates `GitInterop` — `interop:None` ⇒ `vcs.invalid_input`). Lazy-import trigger (AC7.1.2): read procedures resolving an `at`/rev arg that is 40-hex and misses the changeset index call `interop.lazy_import(oid)` once, then re-resolve; `lazy_import` `NotFound` ⇒ `vcs.not_found`.
2. [ ] **`crates/oxidean-vcs/src/activity.rs`** — NEW `server`-gated `ActivityRecorder`: `record(db, repo_id, actor_id, op)` writing ONE `insert_repo_activity` row per mutating op with the C4 mapping (`vcs.repo_enable`/`changeset_create`/`merge`/`rebase`/`ref_update`/`git_import`/`git_export` — `push_type` = the `VCS_ACTIVITY_*` constants in `vcs_types.rs:57-80`, ref/`before`/`after`/`commits_count` nullable per row spec). Never per-object. Called from `VcsApi` post-success only (failures write nothing); facade `skipped()`/`Err` degrades to `warn!`, never fails the op.
3. [ ] **`crates/oxidean-vcs/src/api_error.rs`** (or inside `api.rs`) — `VcsApiError` enum + `to_app_error()` bridge TOTAL per C1 taxonomy: `Disabled→vcs.disabled`, `NotFound→vcs.not_found`, `InvalidInput→vcs.invalid_input`+m, `RefConflict{actual:Option<ChangesetId>}→vcs.ref_conflict`+`VcsRefConflictData`, `Conflict{conflicts}→vcs.conflict`+payload ≤`VCS_CONFLICT_DATA_MAX_ENTRIES` (500, truncate + count), `NoCommonAncestor→vcs.no_common_ancestor`, `QuotaExceeded→vcs.quota_exceeded`, `TooLarge→vcs.too_large`, `ImportState→vcs.import_state`, `Unauthenticated→vcs.unauthenticated`, `Forbidden{verdicts}→vcs.forbidden`+`VcsForbiddenData`, `DbNotConfigured→db.not_configured`, `Internal→rpc.internal`. Engine error bridges reused: `InteropError` per the u6 pinned wire contract (LC6.6 verbatim), `ChangesetError`/`IndexError`/`ObjectStoreError`/`MergeError` mapped total — no `unwrap`, no `String` leak to wire.
4. [ ] **`crates/oxidean-vcs/src/lib.rs`** — `#[cfg(feature = "server")]` block gains `pub mod api;` `pub mod activity;` — never unconditional.
5. [ ] **`crates/oxidean-api/src/vcs.rs`** (NEW host module) — per-procedure RPC handlers: parse `{owner,name}` input → `resolve_repo_for_read` (RepoRead tier) / write-tier resolution → `acl::ensure_not_archived` on EVERY write-class arm (archived-write `repo.archived` GIT-20) → `state.vcs` dispatch → `RpcResponse` envelope. `vcs.repo_enable` resolves `resolve_repo_for_admin`. `VcsCtx` bundle: `{api: Arc<VcsApi>, interop_enabled: bool}` on `AppState`.
6. [ ] **`crates/oxidean-api/src/vcs_protection.rs`** (NEW) — host `ProtectionChecker` impl (C7): `check(repo, refname, old, new, actor_capability)` → delegates to `protection::` verdict path (protection/mod.rs:842-856 `check_ref_update` convention) returning `Result<ProtectionBlockReasons, String>`; injected `Arc<dyn ProtectionChecker>` + `Arc<dyn GitBackend>` (`CliGitBackend`) into `GitInterop::new` at AppState build.
7. [ ] **`crates/oxidean-api/src/rpc.rs`** — replace all 17 `vcs.disabled` stub arms with real dispatch `vcs::<op>(ctx, req.input)`; single-name arm literals preserved (procedures_list_matches_dispatch invariant); `PROCEDURES` unchanged.
8. [ ] **`crates/oxidean-api/src/vcs_routes.rs`** (NEW) — `/api/vcs/{owner}/{name}` nest: `GET /objects/{hash}` → `200 application/octet-stream` + `Cache-Control: immutable, max-age=31536000`, streamed `objects.get`; `POST /objects` → raw blob body, server computes sha256, `200 {ok:true,data:{hash:"sha256:…"}}`, dedup no-op. Auth: PAT (Basic/Bearer)/OAuth access token ONLY — cookie-only/missing ⇒ `401 vcs.unauthenticated` (FR14.1/AC8.5.1); classification via `vcs.object_get`/`vcs.object_put` route aliases (proc_classes.rs:382-383, bearer.rs:735 convention). Errors JSON envelope real statuses: `403 vcs.disabled` (never-enabled only — enabled:false still serves reads) / `403 vcs.forbidden` / `404 vcs.not_found` / `413 vcs.quota_exceeded`. `DefaultBodyLimit::max()` set ABOVE `OXIDEAN_VCS_MAX_OBJECT_BYTES` (512 MiB > 256 MiB) so the typed quota check runs before Axum's untyped 413 (LFS precedent app.rs).
9. [ ] **`crates/oxidean-api/src/app_state.rs` + `main.rs`** — `AppState.vcs: Option<VcsCtx>` built at boot from `OXIDEAN_VCS_*` env (defaults FROZEN per contract table: objects dir `./var/vcs-objects`, gc interval 3600, gc grace 86400, max object 268435456, repo quota 10737418240); invalid/missing objects dir ⇒ `None` + `warn!` (handlers then fail `vcs.disabled`-adjacent typed denial, not panic). `objects`-dir `ObjectStore::open` marker check reuses u3 semantics.
10. [ ] **`crates/oxidean-api/src/repo/activity.rs`** — `VALID_PUSH_TYPES` extended from `VCS_ACTIVITY_KINDS` (vcs_types.rs:73) — allow-list only, no per-op branches.
11. [ ] **GC job** — `crates/oxidean-api/src/jobs/` spawn when `vcs` configured: every `OXIDEAN_VCS_GC_INTERVAL_SECS` calls `ObjectStore::gc(grace)` (`list_orphaned_before(now-grace)` → delete + link cleanup), `info!` stats (scanned/reclaimed bytes/orphans) — ids/counts only.
12. [ ] **Factory-reset sweep** — objects dir joins the factory-reset path (same sweep list as `./var` content stores); documents `OXIDEAN_VCS_OBJECTS_DIR` as swept.
13. [ ] **Operator wiring** — `.env.example` + `docs/CONFIGURATION.md` (five vars, frozen defaults), compose env + `./var` bind for the objects dir, `docs/DEPLOYMENT.md` volume-table row, `.railway/railway.ts` env passthrough. Docs text follows the repo's existing var-doc style.
14. [ ] **`crates/oxidean-api/src/mcp.rs`** — Access-class gating hook for `vcs.*` tool names (descriptor set itself is u9's): tool→procedure class via `proc_classes`, `SessionOnly`/insufficient-class ⇒ denial, `isError` mapping shared with C1 taxonomy.
15. [ ] **Zero edits to u1–u6 production logic** — `vcs_types.rs`/`proc_classes.rs`/`git_interop.rs`/engine internals stay fingerprint-bound; only the recorded surface extensions above.

## Pinned decisions (MUST implement)

- **Flag gate is exactly two-case** (AC1.2.1): absent flag row denies EVERYTHING incl. `vcs.status` — a never-enabled repo looks indistinguishable from disabled on the wire. `enabled:false` denies writes only; reads (`vcs.status`, objects GET, all RepoRead procedures) still serve.
- **`vcs.status` on never-enabled repo ⇒ `vcs.disabled` error envelope**, NOT an `ok:true {enabled:false}` payload (contract-summary C1 note, explicit).
- **Non-idempotency honored**: `vcs.ref_update` retried landed call ⇒ `vcs.ref_conflict{data:{actual}}` with `actual` = stored ref value (AC5.1.3); `changeset_create` byte-identical resubmit ⇒ same id (idempotent); `vcs.conflict` is a typed denial, never `ok:true`.
- **Write-class arms get `ensure_not_archived` BEFORE dispatch** (GIT-20); `repo_enable` is admin-class (resolve_repo_for_admin) and stays open on archived repos like other admin writes only if the host convention allows — verify against `set_archived`'s own resolution and mirror it.
- **Objects POST dedup is a no-op success**, not an error; `PUT`/`POST` shape per pinned FR11.2 (server computes hash — client never trusts a claimed hash).
- **`vcs.object_*` aliases never appear in `PROCEDURES`** (route aliases only, proc_classes.rs:380-383 comment).
- **Cookie auth NEVER reaches `/api/vcs/*`** (AC8.5.1): transport denies before handlers — check how PAT/session is discriminated upstream and deny cookie-only contexts at the route layer; RPC `vcs.*` procedures DO accept session cookies (contract auth-scope note — settings page calls `vcs.status`/`vcs.repo_enable` via `@oxidean/api-client` with `credentials:"include"`).
- **Lazy-import is ONE retry**: resolve-miss on a 40-hex `at` ⇒ `lazy_import` ⇒ re-resolve once; never loop; `interop:None` or `NotFound` ⇒ `vcs.not_found`.
- **Activity rows are op-granular** (C4): one row per successful mutating op; `git_export` `before_oid` = prior branch git oid (null when unlinked); `after_oid`/`commits_count` semantics per the C4 table verbatim.
- **Env defaults are FROZEN** — do not invent different values; `OXIDEAN_VCS_MAX_OBJECT_BYTES` maps to `vcs.quota_exceeded` (NOT `vcs.too_large` — AC3.1.3/AC3.5.3 pinned).
- **413 vs quota**: route `DefaultBodyLimit` must sit strictly above the cap so the typed `vcs.quota_exceeded` path wins; document the two numbers in the route module comment.
- **Git interop absent is not an error state**: `interop:Option` — `git_import`/`git_export`/lazy-trigger degrade to typed denials (`vcs.invalid_input` for explicit calls, plain `vcs.not_found` on read misses), never panic/`Internal` on misconfig.
- **No git-behavior change**: `/api/vcs/*` routes are additive; Git HTTP/SSH transport, protected-review flow, and push hooks untouched (only the `actor_capability` injection landed in u6).
- **Commit shape**: production delta `feat(vcs):` SemVer-prefixed; artifact bookkeeping separate `docs(aidlc)` commit; `make rpc-gen` regenerated client committed with the producer diff (C9 — wire types unchanged, so expected no drift; verify `make rpc-sync-check`).

## Source manifest write-set

`crates/oxidean-vcs/src/api.rs`, `crates/oxidean-vcs/src/activity.rs`, `crates/oxidean-vcs/src/lib.rs`, `crates/oxidean-api/src/vcs.rs`, `crates/oxidean-api/src/vcs_routes.rs`, `crates/oxidean-api/src/vcs_protection.rs`, `crates/oxidean-api/src/rpc.rs`, `crates/oxidean-api/src/app_state.rs`, `crates/oxidean-api/src/main.rs`, `crates/oxidean-api/src/app.rs`, `crates/oxidean-api/src/repo/activity.rs`, `crates/oxidean-api/src/mcp.rs`, `crates/oxidean-api/src/jobs/`, `.env.example`, `docs/CONFIGURATION.md`, `docs/DEPLOYMENT.md`, `.railway/railway.ts`, `docker-compose.yml` (or compose env file actually carrying `./var`).

## Verification gates (order)

1. `cargo test --no-run --workspace` + `cargo test --no-run -p oxidean-vcs --features server` (compile sweep)
2. `cargo nextest run -p oxidean-vcs --features server` (u7 suite + regressions)
3. `cargo nextest run -p oxidean-api` (RPC/route/activity/MCP regressions)
4. `cargo nextest run --workspace` (full sweep)
5. `cargo clippy --workspace --all-targets -- -D warnings` + `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`
6. `cargo fmt --check` + `cargo doc --workspace --no-deps`
7. `cargo machete` + `make dead-code-check`
8. `make rpc-sync-check` (client drift — expected none; regenerate if wire moved)
9. Object-route manual smoke under `make up` / docker compose (docker-first per project memory): PAT-only 401 for cookies, GET 200 octet-stream, POST dedup.

CI legs gating this delta (no ci.yml edits): `rust-build`, `api-rust` shards, `vcs-server`, `dead-code`/`rustfmt`, `rpc-sync-check`.

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "team",
  "ordering": "We implement the change first, then write and run the tests the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing expectations), and the full CI suite must be green before the PR can merge.",
  "scope": "classic",
  "test_strategy": "standard",
  "project_type": "brownfield",
  "applicable_notes": [
    {
      "layer": "org",
      "text": "We treat tests as a first-class deliverable in every Bolt. The specific\nmethodology (TDD, BDD, ATDD, or classic test-after) is affirmed at\npractices-discovery and recorded in `team.md` under this heading with explicit\n`Methodology` and `Ordering` fields; Code Generation resolves those fields\nindependently from coverage, tooling, and scope notes.\n\nWhen no posture has been affirmed, our default per scope is:\n- **Methodology**: test-after\n- **Ordering**: implement each applicable testable layer, then write and run\n  that layer's tests.\n- `mvp`, `enterprise`, `feature`, `infra`, `classic` add an 80% line-coverage\n  floor and CI execution before merge.\n- `bugfix`, `security-patch` add a targeted regression for the specific\n  bug/vulnerability and require the existing suite to remain green.\n- `express` uses the Minimal strategy: requirement-driven unit tests (one per\n  requirement, with a happy-path floor per component); existing tests remain\n  green.\n- `poc`, `refactor`, `workshop` add no extra new-test floor and require the\n  existing suite to remain green.\n\nThe active `Test Strategy` still applies in every scope and determines test\nvolume/types. Scope floors are additive; they never reduce or replace the\nselected strategy.\n\nBuild and Test verifies defined coverage floors and affirmed quality targets;\nthey may not be weakened to make a step pass.\n\nAffirm a stricter posture in `team.md` if the team commits to one."
    },
    {
      "layer": "team",
      "text": "- **Methodology**: test-after\n- **Ordering**: We implement the change first, then write and run the tests\n  the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing\n  expectations), and the full CI suite must be green before the PR can merge.\n- **TDD option**: an in-repo `tdd` skill (`.agents/skills/tdd/`) exists and\n  may be invoked deliberately for a unit, but red-first is not the team's\n  mandated posture.\n- **Coverage**: CI enforces a **weighted composite gate at floor 0.70**\n  (`coverage-weighted` job, `COVERAGE_WEIGHTED_FLOOR`, composed by\n  `scripts/coverage-weighted.sh`): web unit 0.25 + web integration 0.40 (v8\n  line coverage) + e2e 0.35 — where the e2e third is a **7-path presence\n  checklist** (`scripts/coverage-e2e-checklist.sh`), not executed coverage;\n  e2e correctness is enforced by the separate `e2e-stack` / `actions-e2e` /\n  `smoke-protocol` job gates. The composite **consumes web numbers only**:\n  Rust coverage is collected (`cargo-llvm-cov`, artifacts uploaded) but\n  **not gated**, so this intent's new VCS crate lands with no enforced\n  coverage signal — collect-only Rust coverage is recorded debt, and we do\n  not invent a new numeric floor for it (the change-type minimums + green CI\n  carry its testing). Cross-dialect execution in CI is limited to\n  `dialect_probe` per dialect (`db-dialects` job): the ~30 `dialect_*` parity\n  files are `DATABASE_URL`-gated and **skip-pass green** in the `api-rust`\n  shards — full parity is local-capable only, also recorded debt. No\n  performance-test infrastructure exists (no benches, no load harness) —\n  recorded debt for NFR work, not a blocker.\n- **UI coverage is change-aware**: route- and browser-coverage manifests mean\n  every new/changed `apps/web` route needs test evidence, and new/changed\n  high-risk interactive UI (Checkbox / RadioGroup / Select / Switch /\n  portals / `form.Subscribe`) needs Chromium `*.browser.test.tsx` proof —\n  fresh skip-only rows fail CI, and `kind: \"browser\"` entries are\n  source-verified (mount, interact, assert markers). Manifest `skip` rows are\n  the only sanctioned skip: zero `#[ignore]` and zero `it.skip`/`test.skip`\n  in the tree.\n- **Tooling**: cargo-nextest for the Rust workspace (`.config/nextest.toml`,\n  `ci` profile: no fail-fast, 90s slow-timeout; one shared nextest archive\n  from `rust-build` feeds the 5-way `api-rust` shards, `db-dialects`, and\n  archive consumers — one compile per run; shards re-run whole on failure —\n  coarse flake tolerance). Vitest 5 for TS with five projects in `apps/web`\n  (`unit` on node, `integration` on happy-dom with `fileParallelism: false`,\n  `browser` on Playwright Chromium, `e2e-stack` and `e2e-stack-browser` gated\n  on `E2E_STACK=1`) plus `@oxidean/api-client`'s own node-env suite.\n  Playwright drives real-Chromium component and full-stack e2e; wiremock\n  supplies outbound-HTTP fixtures; Rust integration tests open real SQLite\n  tempdir DBs; protocol smokes run real `git`, `ssh`, and `git-lfs` client\n  binaries against a live Compose stack (fail-closed in CI under\n  `SMOKE_REQUIRE_STACK=1`).\n- **Local/CI asymmetry**: `make test` runs only nextest + web\n  unit/integration; browser, stack e2e, and smokes are separate targets — a\n  green local `make test` proves far less than a green CI run, and smoke\n  scripts skip-pass locally without Docker but fail-closed under `CI`.\n- **Test types / minimums**: per change type — pure helper → unit;\n  Query/session UI → happy-dom integration; high-risk UI → Chromium browser\n  test + manifest row; auth/RPC contract → Rust integration and/or stack e2e;\n  dialect-touching DB code → `dialect_*` test (noting the CI-execution gap —\n  the probe is the only enforced cross-dialect run). Suites: colocated Rust\n  units plus `crates/*/tests/` integration (107 API test files), web\n  unit/integration/component tests, stack e2e with Mailpit/OIDC/Resend-WorkOS\n  stubs (`make test-e2e-stack`), forge protocol smokes, compose bring-up\n  smoke per dialect, and an Actions pipeline e2e."
    },
    {
      "layer": "project",
      "text": "- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14) \n\n- Prefer Docker for debugging and testing (docker compose stack, make up/smoke, act for CI lanes) over bare-metal equivalents (learned 2026-10-18)"
    }
  ],
  "obligations": {
    "strategy": "standard",
    "strategy_volume": [
      "Five to eight tests per component.",
      "Unit tests plus integration tests for key boundaries.",
      "Add E2E, performance, or security tests when requirements demand them."
    ],
    "scope_floor": [
      "Keep the existing test suite green.",
      "This scope adds no extra new-test floor beyond the selected test strategy."
    ],
    "combination_rule": "Apply every selected-strategy obligation and every scope-floor obligation; neither replaces the other, and a targeted scope regression may add the narrowest necessary test type beyond the strategy default."
  },
  "plan_profile": {
    "methodology": "test-after",
    "runner_step": "Verify the existing test runner/configuration and record the exact unit-scoped command.",
    "runner_ready_before_first_test": true,
    "testable_layers": [
      "Data model / database behavior",
      "Repository / data access",
      "Business logic",
      "API / endpoint",
      "Frontend behavior"
    ],
    "steps": [
      "Project structure and production configuration skeleton.",
      "Verify the existing test runner/configuration and record the exact unit-scoped command.",
      "Data model / database behavior - implement.",
      "Data model / database behavior - write and run its tests after implementation.",
      "Repository / data access - implement.",
      "Repository / data access - write and run its tests after implementation.",
      "Business logic - implement.",
      "Business logic - write and run its tests after implementation.",
      "API / endpoint - implement.",
      "API / endpoint - write and run its tests after implementation.",
      "Frontend behavior - implement.",
      "Frontend behavior - write and run its tests after implementation.",
      "Environment/build configuration.",
      "Documentation and traceability."
    ]
  },
  "input_sha256": "sha256:304bf27698e8b5f47dff4e16702de3e3c6f286e25e63e22facb420316bac75bc",
  "contract_sha256": "sha256:d7627c3e92082e54b594f33a1ba945e0e072964270fa08e0adcb8fe891ef16c6"
}
```

## Traceability

`traceability.json` maps each deliverable to contract sections (C1–C5/C7/C9 + `OXIDEAN_VCS_*` table), unit-of-work §U7 lines, and requirements FR/AC ids (FR10.x procedures/manifest, FR11.x object routes, FR13.x MCP, FR14.1 cookie denial, AC1.2.1 flag semantics, AC2.x changeset/rebase, AC3.x quota, AC5.1.3 ref_update, AC7.1.2 lazy import, AC8.x taxonomy/authz).
