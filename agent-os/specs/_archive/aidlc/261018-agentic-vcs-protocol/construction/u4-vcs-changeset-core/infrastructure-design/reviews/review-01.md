## Review

**Stage:** infrastructure-design
**Unit:** u4-vcs-changeset-core
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** NOT-READY

### Findings

1. **Major — `traceability.json` uses phantom upstream ID `NFR4.6`** (`infrastructure-design/traceability.json`). No `NFR4.6` exists — the NFR space is flat `NFR1–NFR10` (inception requirements.md:131-137); prior stages map the dialect-parity requirement as `NFR6` (nfr-requirements traceability :96-99 → TD4.3/TD4.7; nfr-design :314-316 → LC4.7). The row's substance is correct — rename `NFR4.6` → `NFR6`.
2. **Minor — stale comment + local repro path for `dialect_vcs` not wired.** (a) ci.yml:814-815 comment ("the three probe tests share one database file") goes stale after widening — update in the same commit. (b) `Makefile:330-331` `db-matrix` runs only `--test dialect_probe`; TESTING.md:107 likewise — post-U4 `make db-matrix` gives zero `dialect_vcs` coverage. Extend `db-matrix` to also run `--test dialect_vcs` or record the deliberate probe-only scope. questions.md Q3's "documented path per docs/TESTING.md" mildly overstates — TESTING.md documents the CI leg and db-matrix, not a compose dialect-test path.
3. **Nit — line citation drift in questions.md.** Q1 cites the leg as ci.yml:763-815; actual span :763-816 (cicd-pipeline.md correct).
4. **Nit — `dialect_vcs` executes-and-skips in the 5 api-rust shards** (unfiltered archive run, no DATABASE_URL). Harmless; artifacts may state it explicitly to pre-empt confusion.
5. **Nit — clippy citation omits `--` before `-D warnings`** in the lane table (ci.yml:334 actual shape: `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`).

### Summary

Every material CI claim verified true against the tree: the widened `-E '… or …'` filter is valid nextest DSL and executes dialect_vcs on all three legs from the unchanged archive; sqlite `--test-threads=1` serializes cross-binary; the fourth ALTER matches 0053's `old_path VARCHAR(512) NULL` exactly and preserves all PK byte budgets; `rust-artifacts-cleanup` needs no wiring; Makefile arms cover the new modules unchanged; the versioning.toml deferral is sound (path bound to u1's source manifest, sha256 8fba2852…, all four reviewed-source TSVs; u1 LC4 anticipated the update). One phantom upstream ID blocks; fix is a one-line traceability correction.

### Verified

| Claim | Evidence | Result |
|---|---|---|
| db-dialects span + filter position | ci.yml:763-816, -E at :810 | ✅ |
| postgres/mysql services health-gated | ci.yml:769-786 | ✅ |
| sqlite cross-binary serialization | ci.yml:816 --test-threads=1 | ✅ |
| `or`-DSL semantics / NO_TESTS_RUN asymmetry | nextest filterset precedent | ✅ |
| dialect_vcs compiles into archive | ci.yml:207-208; oxidean-db no [features] | ✅ |
| skip-pass contract | dialect_probe.rs:8,10-15,20,26 | ✅ |
| migration_parity rides api-rust | migrate.rs:43-69 | ✅ |
| 4th ALTER matches 0053 old_path | mysql/0053:54 VARCHAR(512) NULL | ✅ |
| PK budgets < 3072 | 2640/1164/2176 | ✅ |
| cleanup needs unchanged | ci.yml:929 | ✅ |
| sharpness pin preserved | object_store mod.rs:416/509, gc.rs:429, config.rs:142, paths.rs:64 | ✅ |
| Makefile arms cover | Makefile:237-245,404-408,412-414 | ✅ |
| deps lockfile-resident | Cargo.lock base64 0.23.1/hex 0.4.3; workspace serde; path dep | ✅ |
| versioning.toml bound to u1 manifest | u1 source-manifest.json:10 + reviewed-source TSVs | ✅ |
| TESTING.md:271 unqualified form | verbatim | ✅ |
| facade sentinels + CasOutcome + CAS divergence | vcs.rs:20-23,115-123,1640-1950 | ✅ |
| SR4.11 19-row register, R18 | security-design.md:116-136 | ✅ |
| U4-ND-09 handoff claimed | nfr-design platform.md:43 | ✅ |
