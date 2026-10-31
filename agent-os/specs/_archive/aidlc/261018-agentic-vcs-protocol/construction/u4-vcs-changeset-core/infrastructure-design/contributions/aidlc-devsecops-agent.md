# Contribution — aidlc-devsecops-agent (u4-vcs-changeset-core / infrastructure-design)

Verified every concrete claim in the three artifacts against the actual tree. The design is fundamentally sound — no Blockers, no Majors — but several claims needed correction, one missed wiring item, and one schema question raised and resolved.

## Findings (all integrated)

1. **Minor — phantom `ci-gate` job.** `cicd-pipeline.md` claimed a `ci-gate` aggregator at ci.yml:929; no such job exists — :929 is `rust-artifacts-cleanup`'s `needs`. Fixed: bullet rewritten to cite `rust-artifacts-cleanup` directly (substantive "no change" conclusion unchanged).
2. **Minor — `commit-lint` / "registry completeness" does not cover Cargo.toml dep additions.** `cli.ts check` validates member↔component entries and DAG sanity only. Fixed: lane-table row re-attributes coverage to `cargo machete` + `cargo clippy --all-features` + `cargo doc --features server`.
3. **Minor — missed wiring: `versioning.toml` `depends_on` stale for oxidean-vcs.** `depends_on = []` but the crate has `oxidean-db` (U3) and gains `oxidean-core` (U4); the field drives the dependents-closure bump cascade and no CI gate catches it. Resolution: **deliberately deferred** — `versioning.toml` is bound to another unit's source-manifest fingerprint; editing re-lapses that unit's evidence (observed once on a prior `depends_on` edit, restored by revert). Recorded as a dedicated cicd-pipeline section: land `depends_on = ["oxidean-core","oxidean-db"]` as a `chore(vcs)`/`build(vcs)` commit in the build-and-test stage post-unit-close.
4. **Nit — "all five deps are workspace-pinned" inaccurate.** Only `serde`/`serde_json`/`schemars`/`tokio` are workspace deps; `base64`/`hex` are inline version specs (api/runner precedent) and `oxidean-core` is a path dep. Fixed: "lockfile-resident, no new crates" in lane table + traceability.
5. **Minor — `old_path` collation asymmetry.** `vcs_file_changes.old_path` stayed `utf8mb4_0900_ai_ci` beside a binary `path` — latent ERROR 1267 on mixed predicates. Resolution: fourth `ALTER TABLE vcs_file_changes MODIFY COLUMN old_path VARCHAR(512) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL` added to 0054 in cicd-pipeline.md, questions.md, and nfr-design LC4.7; dialect_vcs coverage extended to old_path round-trips.
6. **Nit — line citation drift + TESTING.md unqualified filter form.** Fixed: job spans ci.yml:763-816; docs note now pins the qualified `or` form.

## Verified

- **Filter syntax valid** — `binary_id(pkg::bin)` + `or` are standard nextest DSL; pinned `nextest@0.9.146` runs both idioms in-tree today (ci.yml:810, :848).
- **Archive inclusion** — oxidean-db has no `[features]`; every `tests/*.rs` target compiles into `nextest-archive` under the unchanged `--features server` archive flag (per-package per RFC-2957). Proven by `dialect_probe` executing from it.
- **sqlite serialization correct** — `--test-threads=1` serializes test processes across *both* binaries on the shared sqlite file; the `SERIAL` mutex is per-process and correctly not relied on cross-binary.
- **No new cleanup consumer** — `db-dialects` already in `rust-artifacts-cleanup.needs` (ci.yml:929).
- **No other missed wiring** — makefile-lint untouched (Makefile unchanged); no migration lint/numbering scripts exist (0040→0043 gap confirms filename-set parity only); `docs/TESTING.md` update planned.
- **Migration DDL sound** — `MODIFY COLUMN` full re-declarations valid; preserves PK membership; no FK touches altered columns; PK byte budgets 2640/1164/2176 all < 3072.
- **Sequencing / NO_TESTS_RUN correct** — a disjunct matching no binary is not an error; only zero total matches fails. Nuance: code-first still runs-and-skips dialect_vcs in unfiltered api-rust shards — "unexercised" means no real-DB coverage, not zero execution.
- **CAS divergence claim accurate** — MySQL `SELECT … FOR UPDATE` under tx (vcs.rs:1835-1950) vs PG/SQLite post-CAS `actual` re-read (:1640-1720, :1738-1814); sentinels :20-23, `CasOutcome` :115-123.
- **migration_parity** — filename `BTreeSet` comparison (migrate.rs:51-69), lib unit test riding the archive in api-rust; `sqlx::migrate!` embeds files at archive build so same-commit landing guarantees 0054 presence.
- **Makefile arms cover new modules unchanged** (:237-246, :404-408, :412-414).
- **Sharpness pin preserved** — all U3 modules `#[cfg(all(test, feature = "server"))]`-gated; U4 suite gated identically keeps the leg's loud-failure signal.
- **Coverage caveat accurate** — `cargo llvm-cov nextest --workspace` is default-features; server suite contributes no coverage until U7 unifies.
- **Runtime config** — `OXIDEAN_VCS_OBJECTS_DIR` only; "no new runtime config" holds.
- **Stage coherence** — nfr-design's platform contribution flagged the filter decision as deliberately deferred (U4-ND-09); this stage correctly claims it. Comment-only parity files remain new in-tree precedent (U4-ND-10 honored — no `SELECT 1;` fallback needed).
