# CI/CD Pipeline — u3-vcs-object-store

One flag on an existing lane, one small new job, no new artifacts. Every existing Rust gate runs **default features**, so without this design the `server`-gated object store compiles nowhere and its tests silently no-op (the DevSecOps finding this stage exists to close).

## Lane coverage

| Change | Lane | Gate |
|---|---|---|
| `crates/oxidean-vcs` `server`-gated module + `#[cfg(all(test, feature = "server"))]` tests | **`cargo nextest archive --workspace --features server`** (ci.yml:207 one-flag edit) → `api-rust` shards execute the suite partitioned across the matrix from the existing archive + **new `vcs-server` leg** (`-E 'package(oxidean-vcs)'` on the same archive) as the dedicated zero-test signal | the suite: put/get/verify round-trips, dedup, quota failures, stream abort, empty objects, GC passes, lock semantics, marker safety, skipped-DB fail-close |
| `crates/oxidean-db/src/vcs.rs` — `vcs_get_object_link` + `vcs_delete_object_if_unlinked` | `api-rust` shards (existing archive — oxidean-db tests are default-feature) | bool-idiom unit tests on sqlite |
| `crates/oxidean-vcs` Cargo.toml feature manifest | `dead-code` + `commit-lint` (`registry completeness` step, ci.yml:158-159) | `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc -p oxidean-vcs --no-deps --features server`; `cargo machete` with `[package.metadata.cargo-machete] ignored` escape for any `dep:`-gated dep it false-positives (api Cargo.toml:103-107 precedent); member↔component already registered (U1) |
| `Makefile` test/dead-code/docs-check lanes | `makefile-lint` + local dev + checkpoint verification | nextest arm `cargo nextest run -p oxidean-vcs --features server`; `cargo test` fallback arm `cargo test -p oxidean-vcs --features server` |

## The `--features server` archive flag (ci.yml:207)

```
cargo nextest archive --workspace --features server \
  --archive-file target/nextest-archive.tar.zst
```

Under `resolver = "2"` + virtual workspace (RFC-2957), unqualified `--features server` enables the feature on every selected package declaring it — only `oxidean-vcs`; other members are unaffected. The single `nextest-archive` artifact then embeds the server-gated test binary; `api-rust` shards execute it for free with zero changes to their steps. `cargo build --workspace --bins` at :205 is unaffected (the crate ships no bin; nothing depends on it yet).

## The `vcs-server` leg (new job)

`needs: [rust-build]`, ubuntu-latest bare (no container — archive pattern, not dev-image). Reuses the api-rust shard preamble verbatim: alias checkout at the baked root (ci.yml:247-250), `dtolnay/rust-toolchain@stable`, `taiki-e/install-action` `nextest@0.9.146` (pin parity :263), `download-artifact` `nextest-archive` → `target/`, then:

```
cargo nextest run --archive-file target/nextest-archive.tar.zst \
  --workspace-remap "${{ github.workspace }}" --profile ci \
  -E 'package(oxidean-vcs)'
```

- **Dedicated zero-test signal** — nextest `--no-tests` default is `fail` (exit `NO_TESTS_RUN`, ≥0.9.85; pinned 0.9.146). If `--features server` is ever dropped from the archive the filter matches nothing → loud failure, not silent green. **Sharpness pin:** the signal is loud only while `oxidean-vcs` ships zero non-`server` tests (all U3 tests are `#[cfg(all(test, feature = "server"))]`); a future ungated test would satisfy the filter even without the feature — keep the whole suite gated or the signal degrades.
- **No partitioning** — one small crate suite; a single run beats 5 shard boots.
- **No compile on the runner** — executes from the same archive (no second artifact → no `jq select` addition needed at :909). **But `vcs-server` is a new consumer of `nextest-archive`:** `rust-artifacts-cleanup`'s `needs` (:893) lists the jobs that download the archive, so `vcs-server` must be added there — otherwise cleanup can race the leg (delete the archive while it is queued) and a failed `vcs-server` could never re-run green, muting the exact signal the lane exists for.
- **db-dialects**: its `-E 'binary_id(oxidean-db::dialect_probe)'` filter (:806) probes migrate/schema only — the new `vcs_*` methods' non-sqlite paths get zero CI execution, exactly like every existing vcs facade method (recorded debt, team.md); their sqlite unit tests ride the default archive in api-rust shards. `vcs-server` needs no dialect wiring.
- **coverage-rust**: `cargo llvm-cov nextest --workspace` (Makefile:353) is default-features — the server suite won't contribute to collected (ungated) Rust coverage until U7's dep edge unifies features; flag for U7 planning, not a U3 gate.

## Makefile deltas (LC3.6 carried)

- `test` (Makefile:237-243): nextest arm `cargo nextest run -p oxidean-vcs --features server`; fallback arm `cargo test -p oxidean-vcs --features server`.
- `dead-code-check` (Makefile:405): `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`.
- `docs-check` (Makefile:410): `RUSTDOCFLAGS="-D warnings" cargo doc -p oxidean-vcs --no-deps --features server`.
- Doc tests: no doc examples in the server module (pinned in LC3.6 — nextest never runs doc tests and `cargo test --doc` is default-features).

## Feature-unification nuance

The `-p --features server` Makefile invocations are load-bearing **only until U7** adds `oxidean-vcs = { features = ["server"] }` to a host crate: resolver-2 then compiles the gated code workspace-wide anyway. Keep them regardless — they guard U3–U6 where no host edge exists, and `--all-features` clippy stays meaningful forever. The CI flag likewise stays: even post-U7 it guarantees the suite is in the archive regardless of which crate edges exist.

## Sequencing constraint

The Makefile + ci.yml edits land **in the same commit** as the U3 implementation — `vcs-server` would red on a tree without the module, and default-only lanes would ship untested code if the implementation landed first. Follows U1/U2's single-landed-tree rule. Docs: add the `vcs-server` row to the CI table in docs/TESTING.md:258-259 (house convention for new lanes).

## Rollback

`git revert` — one CI flag + one small job + Makefile lines + a crate feature; deleting them reverts coverage to pre-U3 defaults with no state to unwind. Runtime config (`OXIDEAN_VCS_*` env) is operator-side and unaffected.
