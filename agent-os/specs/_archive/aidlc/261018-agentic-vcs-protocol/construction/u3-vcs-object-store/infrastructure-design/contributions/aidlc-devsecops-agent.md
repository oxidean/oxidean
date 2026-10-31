# Support Contribution — aidlc-devsecops-agent
**Stage:** infrastructure-design | **Unit:** u3-vcs-object-store | **Verdict:** NEEDS CHANGES (2 majors; both integrated — design replaced)

1. **[MAJOR] A strictly cheaper design was not analyzed: fold `server` into the existing workspace archive.** `cargo nextest archive --workspace --features server` at ci.yml:207 achieves the coverage with zero new artifacts/jobs — virtual workspace + `resolver = "2"` (RFC-2957) enables it only on packages declaring it (oxidean-vcs). A dedicated zero-test signal survives via `-E 'package(oxidean-vcs)'` on the same archive. **Fixed: design replaced — one flag on :207 + `vcs-server` `-E` leg; the two-archive design documented as considered/rejected in Q1.**
2. **[MAJOR] `rust-artifacts-cleanup` wiring was missing** — ci.yml:891-915 `needs` + jq `select(.name == "rust-bins" or .name == "nextest-archive")` would have orphaned `nextest-archive-vcs-server` or deleted it under the job. **Evaporated under the chosen design — single shared artifact, no cleanup wiring; noted in cicd-pipeline.md.**
3. **[MINOR] Facade-method dialect parity oversold** — `db-dialects` filters `-E 'binary_id(oxidean-db::dialect_probe)'` (:806) — migrate/schema probe only, no per-method parity. **Fixed: honest note in cicd-pipeline.md + NFR6 row.**
4. **[MINOR] Server suite invisible to Rust coverage** — `coverage-rust` `cargo llvm-cov nextest --workspace` is default-features; collected-only, ungated. **Fixed: noted as U7 planning flag.**

## Verifications
- `cargo nextest archive -p <pkg> --features` produces valid runnable archives; `--workspace-remap` works package-count-agnostic; alias symlink still needed (`CARGO_MANIFEST_DIR` baked).
- `vcs-server` on ubuntu-latest bare: `contents: read` only; dead-code in dev image correct as proposed.
- `cargo build --workspace --bins` does NOT touch oxidean-vcs (no `[[bin]]`, no dependents, feature-gated) — "compiles nowhere" accurate.
- Makefile lane shapes verified: `test` :237-243 if/else arms fit verbatim; `dead-code-check` :402-405 / `docs-check` :409-410 sequential lines fit.
- cargo-machete is a textual use-scanner — `dep:`-gated deps flag only when never textually named; `ignored` hedge accurate (api Cargo.toml:103-107 `["prost","prost-build"]`).
- `.config/nextest.toml` `ci` profile exists (:8-12). nextest `--no-tests` default `fail` since 0.9.85; pinned 0.9.146 — dedicated-leg zero-test signal real.
- `-p --features server` in-shard compile correctly rejected (server tests dev-dep `oxidean-db`+`tempfile` → fresh sqlx/tokio build, no cooked `/ci/target-dev`).
- `vcs_get_object_link`/`vcs_delete_object_if_unlinked` genuinely absent — legitimately future work; versioning.toml:89-97 registers oxidean-vcs (U1).

## Suggestions adopted
- Prefer `--features server` on the existing archive; keep `-p --features server` Makefile lanes (load-bearing pre-U7). **Both adopted.**
- If second-archive shipped: cleanup `needs` + jq select. **Moot under chosen design.**
- `ls -lh` the archive + `if-no-files-found: error` discipline for any upload step. **N/A — no new upload step.**
- Drop free-disk step in the small leg; harmless either way. **Noted; preamble kept verbatim for consistency.**
