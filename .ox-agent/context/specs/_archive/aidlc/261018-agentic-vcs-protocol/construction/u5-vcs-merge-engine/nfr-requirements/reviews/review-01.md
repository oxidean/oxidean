## Review

**Verdict:** NOT-READY
**Stage:** nfr-requirements
**Unit:** u5-vcs-merge-engine
**Iteration:** 1
**Reviewer:** aidlc-architecture-reviewer-agent

### Findings

All 10 pass-1 fixes verified applied. New:

- **[BLOCKER] F1** — WF5 (rebase) never got the reorder: manifest_diff + cap + input pre-check still run AFTER pass-2 puts (`functional-spec.md:58-59`) — identical post-checkpoint link-leak class on the rebase path.
- **[Major] F2** — WF1.9's author mirror under-specifies `compose_author` (nonempty/trim + `<>`/`\n`/`\r`/controls/0x7f, composed ≤255 **chars**; message bound is **bytes**) — a literal mirror passes `""`/`"  "` and `create` rejects post-put.
- **[Major] F3** — TD5.3 still says "validated once there — no re-check" — contradicts WF1.1/WF1.9; an implementer skips the mirror.
- [Minor] F4 — `QuotaExceeded(Object)` is a deterministic post-checkpoint abort under `merge_blob_max > max_object_bytes` misconfig — pin `merge_blob_max ≤ max_object_bytes` in MergeCaps validation or WF1.9.
- [Nit] F5 — `PutOutcome.hash == recorded sha256` never asserted — pin `assert ⇒ Internal` on mismatch.
- [Nit] F6 — `PathNotFound` dead-arm parenthetical: `compute_blame` also emits it (`read_index.rs:1525`); engine calls neither.
- [Nit] F7 — WF4.1 reads as if `split_lines` returns the tuple; pin `ends_nl` caller-derived (`bytes.last() == Some(&b'\n')`; empty ⇒ false).
- [Nit] F8 — residual precision: `ParentNotFound` (repo-delete TOCTOU) / `ObjectNotFound` sentinel decodes also post-checkpoint-reachable — name them in SR5.11.

### Summary

Two-pass mechanism verified sound (put mints `bytes_to_hex(Sha256)`; local mint byte-identical; merge3 deterministic; retry converges via link-membership probe). Merge-path reorder correct. Rebase ordering, mirror completeness, and the TD5.3 contradiction remain.
