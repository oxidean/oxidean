## Review

**Stage:** nfr-design
**Unit:** u3-vcs-object-store
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-01 | major | Pinned probe SQL uses `repository_id`; the column is `repo_id` on all three dialects (facade INSERT vcs.rs:605, `l.repo_id` :649). `repository_id` is the lfs_object_links column — copied with the has_lfs_link shape. Verbatim codegen yields a runtime prepare failure on every PUT | logical-components.md LC3.4 | Unresolved |
| R-02 | major | Marker gate `!marker ⇒ Config` fires on an absent objects dir too; BR3.7/WF4.4 pin `gc_once` early-returns zero stats on absent dir (LFS lfs_gc.rs:39-41 precedent; TD3.7 pins the test). Refusal must be conditioned on non-empty && !marker | security-design.md SD3.5 | Unresolved |
| R-03 | minor | `new()` write-order wedge: `create_dir_all(dir + tmp)` before marker write — a crash leaves a tmp-only dir → permanent Config refusal next boot. Marker before tmp (or exempt self-owned names) | SD3.8 / LC3.2 | Unresolved |
| R-04 | minor | Eager-cleanup branch doesn't restate the bool gate — unconditional "delete + unlink" strands a live link over missing bytes if a link races in; gate the unlink on the delete returning true | SD3.3 | Unresolved |
| R-05 | minor | `≤ i64::MAX` grace bound doesn't close the Duration hole — chrono ms repr overflows above ~i64::MAX/1000. Bound grace `≤ i64::MAX/1000` or `try_seconds`; interval feeds std::time::Duration, unbounded | SD3.9-R10 | Unresolved |
| R-06 | minor | Lane additions drop `-D warnings` parity — clippy `-- -D warnings` (:405) and `RUSTDOCFLAGS="-D warnings"` (:410) must carry onto the new lines | LC3.6 | Unresolved |
| R-07 | minor | Traceability gaps: BR3.3 (dedup link-ensure, SR3.7 cites it) and BR3.7 (env contract/absent-dir) absent while nine sibling BRs traced; TD3.1 "cargo-tree enforcement" has no LC pin; oxidean-core deferral absent from SD3.10 register | traceability.json | Unresolved |
| R-08 | minor | SR3.5 → "counts only" overclaim — per-object warn logs in SD3.5 unpinned; add: log %hash/%error, never %path/env | security-design.md SD3.5 | Unresolved |
| R-09 | nit | WF3 is the links-tx workflow; the stale "GC pass 2" numbering lives at entities.md:31 / WF4 ordering — fix the pointer | SD3.10 | Unresolved |
| R-10 | nit | `put<S,E>` omits the `E: Display` bound needed for `Stream(String)` (LFS store.rs:43 has it) | LC3.2 | Unresolved |
| R-11 | nit | `Io`/`Internal` payloads unpinned — pin `Io(std::io::Error)` + what `Internal` carries | LC3.3 | Unresolved |
| R-12 | nit | `Ok(Stats::skipped)` reads as a variant; `skipped` is a bool field — write `Ok(stats)` with `skipped = true` | SD3.4 | Unresolved |
| R-13 | nit | Register could carry unbounded `list_orphaned_objects_before` Vec (memory + lock-duration spike; upstream-accepted at TD3.4) as R11 | SD3.9 | Unresolved |
| R-14 | nit | `git/cli.rs:430` is `crates/oxidean-git/src/cli.rs` — tighten citation | SD3.8 | Unresolved |
| R-15 | nit | Pin io::NotFound-tolerant `remove_file` in both sweep passes (LFS store.rs:97-101 precedent): a vanished file isn't an error event | SD3.5 | Unresolved |

### Summary

NOT-READY: two majors in pinned, verbatim-copyable surfaces (exact facade SQL column; marker-gate vs absent-dir ordering). Both one-line fixes plus thirteen cheap minors. Verified integrations: facade surface verbatim-accurate (vcs.rs:2287–2333); both additions genuinely absent and fit bool idioms; dual-NOT-EXISTS dialect-portable with serving indexes; is_skipped/require_pool/Database:Clone confirmed; LFS precedents confirmed (no permissions, no fsync — LockGuard Drop strictly better than lfs_gc.rs:79-80); cfg-gate mechanics, dep pins, wire codes, Makefile lanes, ci.yml:207 archive all verified; enum surface complete incl. Config correction; PutOutcome bare-hash; VcsGcStats skipped/errors; lock-steal R8 correctly integrated.
