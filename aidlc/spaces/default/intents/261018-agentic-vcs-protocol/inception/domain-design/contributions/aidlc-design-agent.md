**Collaborator:** aidlc-design-agent

## Contribution

1. **[blocking] Flag-gate scope is ambiguous and, read literally, violates AC1.2.1 + the toggle contract.** `components.md` VcsApi §behaviour: *"every procedure checks RepoSurfaceFlag and fails closed with vcs.disabled (never-enabled and disabled repos share the path)"*. Upstream pins two *different* scopes: AC1.2.1 — disabled repo (`enabled:false` row) denies **writes only**, reads still succeed; interaction-spec C1 + mockups S1 — never-enabled (**absent** row) denies everything, so `vcs.status` returns the `vcs.disabled` *error envelope* → toggle renders unchecked-default. The only model satisfying both: absent row → deny all; `enabled=false` → deny writes only. "Every procedure fails closed" literally implemented = disabled repos deny `vcs.log`/object GET — breaking US1.2 and the toggle's own copy *"existing history stays readable"*.

2. **[blocking] Activity-feed UX (S5, AC2.7.x) has three unowned consumer-side seams; traceability maps US2.7 → ActivityRecorder only.**
   a. `VALID_PUSH_TYPES` is a **private const in `oxidean-api`** (`src/repo/activity.rs:20–27`) and gates the `push_type` filter param (`activity.rs:397–410` → `rpc.bad_input`). The crate-side ActivityRecorder cannot see it; AC2.7.3's "extend the allow-list" pins no owner/location (core? shared?). Without extension, kind-filtering 400s — "actor/kind filterable" broken.
   b. `actionCopy` (`apps/web/src/components/repo/repo-activity-row.tsrx:11–33`) falls through unknown `push_type` to `"pushed to {branch}"` and renders `commit_message` as a link via `commitHref` → `/{owner}/{repo}/commit/{sha}` (`repo-browse.ts:139–141`). A `cs_…` id passes `hasCommitOid` (row:40–42,56) → **git-commit 404 link + mislabeled row**. No component owns this renderer update; VcsToggle is the only web component catalogued.
   c. Filter chips hardcode the six git types (`$owner.$repo.activity.tsrx:66–74`) — vcs.* kinds unfilterable in UI.
   d. `insert_repo_activity` is push-shaped (`oxidean-db/lib.rs:4525–4537`: `ref_name, before_oid, after_oid, commits_count, pr_number`). components.md names no per-op field mapping (e.g. `changeset_create` touches no ref — `ref_name`=? `before/after`=?; `repo_enable`=?). Pin an op→row mapping table or the feed renders garbage.

3. **[blocking] Archived-write denial (AC1.3.3) has no owner.** Host convention: each write handler calls `acl::ensure_not_archived` → typed `repo.archived` (`repo/acl.rs:435–438`; call sites `repo/mod.rs:1551`, `commit_status.rs:81`, `git_smart_http.rs`). It is **not** part of class resolution and is api-private — the crate can't call it. VcsApi's listed gates (flag, authz class, input validation) omit it. Pin where the archived check lands (host-side write-class pre-gate, or `archived` bit in the resolved context) and which code it returns (`repo.archived` precedent vs `vcs.forbidden`) — the toggle's disabled-input(archived) state and CLI error text both ride on it.

4. **[suggestion] Pin WorkingCopy's result-report contract.** S2.6's pinned lines — *"materialized 37 files (skipped 1,204 blobs outside filter)"*, *"(0 fetched — all cached)"* — require a `CheckoutResult` carrying `{files_written, blobs_fetched, cache_hits, skipped_outside_filter}`. §responsibilities names only "sparse fetch planning + materialization"; CliSurface can't render the pinned output without it.

5. **[suggestion] Hard `cli→vcs` dep drags server deps into `ox`.** `oxidean-cli/Cargo.toml` today: `oxidean-core` + thin crates only — no `oxidean-db`/sqlx. WorkingCopy/CliSurface need no DB; gate the server half (stores + VcsPersistence) behind a cargo feature or the CLI transitively compiles sqlx×3 + `oxidean-git`. ADR-007's "+1 dep" understates the regression.

6. **[nit] McpSurface "descriptors mirror its procedures 1:1"** contradicts its own absence list (12 tools ⊂ 17 procs). Say "1:1 for the FR13.1 subset".

7. **[nit] The interaction-spec's disabled-input "lacks admin class" prong is unreachable.** The settings page gates on `can_admin` wholesale — non-admins never render the control. Keep the archived prong; record the dead state so it isn't test-targeted.

8. **[nit] `third-party-api` labels on WorkingCopy/CliSurface's RPC transport** — it's the forge's own first-party API; `other`/host-provided fits the actual threat model.

9. **[nit] Zero-state CLI lines** (interaction-spec C2: `no changesets`, `[]`) aren't named in CliSurface responsibilities — implied by human-text ownership but worth one explicit line since the spec pins exact strings.

## Positions

- **Two-case flag semantics:** absent `RepoSurfaceFlag` row → deny all procedures incl. `vcs.status → vcs.disabled`; `enabled=false` → deny writes only. This split *is* the toggle's never-enabled→unchecked-default contract and US1.2's lifecycle promise.
- **Single gate at VcsApi:** flag + class + typed taxonomy in one place; ObjectStore's "no auth inside — VcsApi gates first" keeps object routes and RPC on one denial path.
- **McpSurface invariants:** separate `owner`/`name` args (fail-open today if absent); 12-tool set + deliberate absences per FR13.1; `isError` mapping stays host-side.
- **CliSurface output discipline:** `Out::Rpc`, `ox: <code>: <message>` stderr, `error_exit_code` with the `vcs.unauthenticated`/`vcs.forbidden` → 3 match-list extension, local WorkingCopy errors as `ox: <local-code>` never `vcs.*`.
- **WorkingCopy purity:** host-injected transport, want-fetch only, bounded hash-verified cache, typed local errors, never server state.
- **GitInterop `ProtectionChecker` port:** identical verdict path/text to `git push`; `--force` overrides fast-forward only.
- **`ref` wire field (ADR-009):** `#[serde(rename="ref")]` precedent verified.
- **`vcs.status` provisional (ADR-009) is fine** — but the frozen schema must ship `enabled` plus the `vcs.disabled`-error-on-never-enabled behaviour.
- **VcsToggle stays the only web control** (O5) — noting it will only ever render inside an admin-gated page.
