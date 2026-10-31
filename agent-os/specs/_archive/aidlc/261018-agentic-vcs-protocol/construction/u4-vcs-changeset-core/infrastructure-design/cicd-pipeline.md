# CI/CD Pipeline — u4-vcs-changeset-core

One filter widening on an existing leg; no new jobs, services, or artifacts. U3 landed the `server`-feature machinery (`--features server` archive flag, `vcs-server` leg, Makefile arms, `rust-artifacts-cleanup` wiring) — U4's crate suite rides all of it unchanged. The new surface is `dialect_vcs.rs`, U4 being the first unit whose correctness is **dialect-divergent**: the MySQL `0054` collation/width migration and the per-dialect CAS `actual`-read paths are exactly the behaviors that break per-engine and nowhere else.

## Lane coverage

| Change | Lane | Gate |
|---|---|---|
| `crates/oxidean-vcs` `server`-gated modules (`changeset_store`, `ref_registry`, `read_index`) + tests | **`vcs-server` leg** (existing — `-E 'package(oxidean-vcs)'` on the `nextest-archive`) | the U4 suite: canonical-ID golden vectors, idempotent resubmit, concurrent same-parent creates, CAS arms, ref-name/ID-shape rejection, cursor pagination, tree materialization + crash-resume, blame + rename chains, merge-base cases, frontier caps, sentinel non-match, `DbNotConfigured` |
| `src/text_diff.rs` | `vcs-server` leg (same filter — module compiles under `server`) | byte-line golden vectors incl. `\0`-bearing lines, `\r\n` verbatim, LINE_CAP coarse-fallback |
| `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0054_vcs_key_collation.sql` | `migration_parity` unit test (rides default archive in api-rust shards — filename-set parity) + **`db-dialects` leg** (real `db.migrate()` per engine; MySQL executes the `MODIFY COLUMN` DDL, pg/sqlite the comment-only files) | schema applies clean on all three; MySQL post-migration collation is `utf8mb4_bin` |
| `crates/oxidean-db/tests/dialect_vcs.rs` | **`db-dialects` leg — widened `-E` filter** (below) | per-dialect facade coverage: `vcs_create_changeset_tx`, all CAS arms incl. MySQL atomic vs pg/sqlite post-CAS `actual` reads, sentinel strings, case-variant ref/path under 0054 collation |
| `crates/oxidean-vcs` Cargo.toml `server` feature dep additions (`serde`, `serde_json`, `base64`, `hex`, `oxidean-core`) | `dead-code` | existing arms cover it: `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`; `cargo doc -p oxidean-vcs --no-deps --features server`; `cargo machete` (all five are lockfile-resident — `serde`/`serde_json` are workspace deps, `base64` 0.23/`hex` 0.4 already in Cargo.lock via api/runner, `oxidean-core` a path dep — **no new crates**; `commit-lint`'s registry-completeness step validates member↔component only, not dep graphs) |

## The `db-dialects` filter widening (ci.yml:763-816)

Current:

```
DATABASE_URL="$url" OXIDEAN_DB_DIALECT="$dialect" \
  cargo nextest run --archive-file target/nextest-archive.tar.zst \
    --workspace-remap "${{ github.workspace }}" \
    --profile ci -E 'binary_id(oxidean-db::dialect_probe)' "$@"
```

Proposed — one expression change inside `probe()`:

```
    --profile ci -E 'binary_id(oxidean-db::dialect_probe) or binary_id(oxidean-db::dialect_vcs)' "$@"
```

- **Same archive** — `dialect_vcs` is a default-feature `oxidean-db` test binary; it compiles into `nextest-archive` under the unchanged `cargo nextest archive --workspace --features server` (the flag only affects `oxidean-vcs`; RFC-2957).
- **Same services, same gate** — postgres + mysql services are already booted and health-gated; the `probe()` function already iterates all three dialects with `DATABASE_URL` per leg. `dialect_vcs` uses `dialect_probe`'s exact contract: `DATABASE_URL` unset → `eprintln!` skip-pass (local runs unaffected), `db.migrate()` applies the chain including 0054, `SERIAL` mutex + the sqlite leg's `--test-threads=1` keep single-writer sqlite serialized.
- **Why not a separate leg** — a dedicated runner for a two-binary filter delta would re-pay the service boot + archive download for identical coverage.
- **No cleanup wiring** — `rust-artifacts-cleanup`'s `needs` tracks jobs that *download* the archive; `db-dialects` is already listed (ci.yml:929). No new consumer.
- **`vcs-server` sharpness pin carries** — U4's whole suite stays `#[cfg(all(test, feature = "server"))]`-gated, preserving the leg's empty-filter → `NO_TESTS_RUN` loud-failure signal (cicd u3 pin).

## What U4 does NOT change

- **`vcs-server` leg** — zero edits; the package filter picks up U4's tests automatically.
- **Makefile** — zero edits; `cargo nextest run -p oxidean-vcs --features server` (nextest + `cargo test` fallback arms), `dead-code-check`, `docs-check` all cover the new modules.
- **coverage-rust** — same default-features caveat as U3 (server-gated suite contributes no collected coverage until U7 unifies features); recorded, not a U4 gate.
- **e2e-stack / actions-e2e / rpc-sync** — untouched; U4 ships no wire surface (U7 owns it).
- **`rust-artifacts-cleanup`** — `db-dialects` is already in its `needs` (ci.yml:929); the widened filter creates no new archive consumer, so no wiring change.
- **api-rust shards** — `dialect_vcs` will also execute-and-skip there (unfiltered archive run, no `DATABASE_URL`): a sub-second no-op per shard, identical to `dialect_probe`'s current behavior outside its dedicated leg; expected, not a defect.

## Migration safety note

`0054_vcs_key_collation.sql` is **additive-only in CI terms** but **irreversible-shape on MySQL** — four `MODIFY COLUMN` re-declarations: `vcs_derived_index.index_key` → `VARCHAR(560) utf8mb4_bin`, `vcs_refs.name` → `utf8mb4_bin`, `vcs_file_changes.path` → `utf8mb4_bin`, **and `vcs_file_changes.old_path` → `utf8mb4_bin`** (INF finding: leaving `old_path` on `utf8mb4_0900_ai_ci` gives rename chains a case-sensitive `path` but case-insensitive `old_path`, and any future predicate mixing them raises ERROR 1267 illegal-collation-mix; the column is unindexed so conversion is cheap — conversion keeps the path pair symmetric). Fresh-schema CI applies it from scratch on every dialect leg — the failure mode it guards (case-insensitive merge of case-variant refs/paths) is exercised by the widened dialect test, not by a rollback path. The pg/sqlite files are comment-only `0054` stubs required by `migration_parity`'s filename-set check. MODIFY preserves PK membership; no FK touches the altered columns; MySQL PK byte budget `(36+64+560)×4 = 2640 < 3072` holds.

## Fingerprint-bound edits — recorded deferral

Two small edits are **deliberately deferred out of U4's commit** because both paths are bound to other units' source-manifest fingerprints — editing them re-lapses those units' recorded evidence (observed once already on a prior `depends_on` edit, restored by revert). The distinguishing rule: the commit's edits to bound paths (ci.yml, TESTING.md — bound to u3; Cargo.lock — bound to u1) are part of U4's **claimed feature write-set**, recorded in u4's own source manifest; the resulting re-verification of lapsed unit evidence is accepted mechanical cost because the feature genuinely requires those files. The deferred pair are **out-of-scope housekeeping** — U4 needs nothing from them, so they stay off its claim surface entirely. Land both as a `chore(vcs)`/`build(vcs)` commit in the build-and-test stage — post-unit-close, when the unit walk no longer re-evaluates earlier checkpoints and the edits bind to nothing live:

1. **`versioning.toml`** — `oxidean-vcs` lists `depends_on = []`, but the crate already has `oxidean-db` (U3, optional dep) and U4 adds `oxidean-core`; `depends_on` drives the dependents-closure bump cascade (scripts/versioning/lib/plan.ts:60-71). Set `depends_on = ["oxidean-core", "oxidean-db"]`. No CI gate catches the staleness (the check step validates member↔component entries and `depends_on` DAG structural sanity, but not dep-graph-vs-Cargo-manifest fidelity) — this note is the tracking mechanism.
2. **`Makefile` `db-matrix`** — extend to also run `cargo test -p oxidean-db --test dialect_vcs -- --nocapture` so local `DATABASE_URL` repro covers the vcs dialect suite (reviewer finding; `Makefile` is bound to u3's source manifest). The un-extended one-liner (`DATABASE_URL=… cargo test -p oxidean-db --test dialect_vcs`) works today regardless; the TESTING.md update in the U4 feature commit (sequencing constraint, below) documents it.

## Sequencing constraint

The `db-dialects` filter edit lands **in the same commit** as `dialect_vcs.rs` + the 0054 migrations + the U4 crate code + the ci.yml:814-815 comment refresh (stale "the three probe tests" → both binaries share the file) — a widened filter matching a not-yet-existing binary would yield `NO_TESTS_RUN` on the `dialect_vcs` term only if it were the sole filter term; with `or` the probe half still matches, so the risk is asymmetric: filter-first silently under-covers (probe passes, vcs absent → `dialect_probe` term still runs, nextest succeeds), code-first leaves the test file unexercised until the filter lands. Single-commit landing removes both windows. Follows the U1–U3 single-landed-tree rule. Docs: add `dialect_vcs` to the db-dialects description in docs/TESTING.md (house convention) — use the qualified form `binary_id(oxidean-db::dialect_probe) or binary_id(oxidean-db::dialect_vcs)` matching ci.yml (TESTING.md:271 currently writes the unqualified `binary_id(dialect_probe)`).

## Rollback

`git revert` — one filter expression + test file + three migration files + crate modules. The migrations are append-only (no `migrate undo` support exists; rollback is a new forward migration if ever needed — same posture as all prior migrations).
