# NFR Requirements — Questions (u1-vcs-wire-foundation)

Library-kind unit: wire DTOs, error taxonomy, proc_classes entries, activity constants, versioning registration. Performance/scalability/reliability/observability categories don't apply to a type-definition unit — the produce set is security-requirements + tech-stack-decisions + traceability. The remaining question space is narrow; most NFR constraints are already pinned by contract-summary C1/C9 and codebase conventions.

**Q1 — Error-payload leakage bound.** `AppError.data` payloads (ref_conflict `actual`, conflict lists, verdicts) — any redaction or size cap beyond the typed shapes?

[Answer]: No redaction needed — all payload fields are repository-internal identifiers the caller is already authorized to see (the authz check precedes payload construction). One bound: `conflict.conflicts` list caps at 500 entries (the C1 `limits.pagination` ≤500 bound) — a larger conflict set truncates with a dedicated `data.truncated: true` flag (house convention, never overloading `detail`), because a pathological merge shouldn't mint an unbounded error body.

**Q2 — proc_classes coverage enforcement.** What proves no `vcs.*` name reaches dispatch unclassified?

[Answer]: The existing fail-closed `_ => SessionOnly` fallthrough plus an extended coverage test asserting (a) every `vcs.*` entry in PROCEDURES has a non-SessionOnly class, (b) every `vcs.*` PROCEDURES entry has a dispatch arm, (c) the two route aliases (`vcs.object_get`/`vcs.object_put`) are classified and absent from PROCEDURES. SessionOnly still denies token callers; session contexts reach the handler which itself gates on the flag.

**Q3 — Wire-type compile-time safety.** Any NFR on the DTO definitions beyond serde correctness?

[Answer]: Two: (a) every wire DTO derives `schemars::JsonSchema` alongside Serialize/Deserialize — the codebase convention (repo_types.rs) keeps docs/diagnostics honest; (b) `deny_unknown_fields` stays absent everywhere (BR1.6 additive-evolution), and `VcsErrorCode`/`ActivityKind` enums carry per-variant renames so the wire strings are the contract, not the Rust identifiers.
