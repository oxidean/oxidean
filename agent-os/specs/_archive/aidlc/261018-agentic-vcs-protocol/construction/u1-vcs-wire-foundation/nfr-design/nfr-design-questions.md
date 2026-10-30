# NFR Design — Questions (u1-vcs-wire-foundation)

NFR-requirements pinned SR1–SR5 + the tech-stack rows. Design questions remaining: how the security design and the unit's logical decomposition actually arrange in code.

**Q1 — Where does the classification truth live?** The pinned 17+2 name→tier map — hand-maintained literal match arms in `proc_classes.rs` (existing style) or a data table?

[Answer]: Literal match arms grouped in one `vcs.*` block, matching the file's existing style (the `classify_pat_proc` match body is ~240 of the file's 639 lines). A data table adds an indirection the file doesn't have elsewhere; the coverage test — not the encoding — is the safety mechanism, and it asserts the full 19-name mapping either way.

**Q2 — How do the `Nullable<T>` and `VcsErrorCode` helpers shape up in `vcs_types.rs`?**

[Answer]: `Nullable<T>` is a small private enum `Null | Value(T)` + custom `Deserialize` + `Deref`-free accessors (`is_null()`, `value()`); `VcsErrorCode` carries per-variant `serde(rename)` plus `impl From<VcsErrorCode> for String` + `as_str()`. Both live at the top of `vcs_types.rs` before the per-procedure DTO blocks — no new module file, one file keeps the wire contract readable in one pass (entities.md §"vcs_types.rs layout" pins this shape).

**Q3 — Does the security design need its own threat model, or does it project the contract's?**

[Answer]: Projects it. security-design.md maps each SR1–SR5 requirement to the concrete mechanism (fail-closed match arm, domain-vs-transport code split, bounded data payloads, `Nullable<T>` decode, unvalidated-DTO-by-design + u7 handler bounds) — the threat model itself lives in contract-summary C1/C2/C9 and the shared auth paths this unit rides.
