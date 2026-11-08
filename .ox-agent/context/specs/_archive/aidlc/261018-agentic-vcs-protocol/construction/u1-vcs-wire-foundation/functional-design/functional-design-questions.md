# Functional Design — u1-vcs-wire-foundation — Questions

Library-kind unit. Most wire decisions are already frozen by `contract-summary.md` C1/C9 — these questions resolve the remaining representation choices.

**Q1 — Error-code representation.** Are `vcs.*` codes Rust `const &str` values collected in a module, or a closed enum serialized as strings?

[Answer]: Closed enum `VcsErrorCode` with per-variant `#[serde(rename = "vcs.<code>")]` on the dotted wire strings (container `snake_case` would emit `vcs_disabled`, not `vcs.disabled`), plus a `const` module is rejected — an enum gives exhaustiveness checks on `data` payloads and converts to `AppError::code` string at the boundary, matching the existing `AppError{code: String}` shape.

**Q2 — Field optionality discipline.** How do optional wire fields serialize?

[Answer]: `Option<T>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` — absent-not-null on the wire, matching existing `oxidean-core` DTOs. The exception is `ref_update`'s `expected_old`/`new`, where absent vs null is load-bearing (`null` = create-only/delete, absent must be a decode error, not a silent delete): those two fields use a `Nullable<ChangesetId>` newtype (enum `{Null, Value(T)}` + custom `Deserialize`) — a non-`Option` field, so serde raises `missing_field` on absence instead of collapsing it to `None`.

**Q3 — Enum/union input fields.** `vcs.log.from` (ref|changeset_id|git_oid union), `changeset_create` change kinds, and mode octal strings — tagged enum or string + server validation?

[Answer]: String fields with server-side parse/validate (the union is positional, not tagged — `from` accepts all three forms and resolves by shape/lookup); `kind`/`mode` use validated newtypes at the domain boundary, plain strings on the wire DTO. Matches contract notation `"ref_name|changeset_id|git_oid"`.

**Q4 — proc_classes placement.** Where do the 17 `vcs.*` classifications land?

[Answer]: `classify_pat_proc`'s match list in `oxidean-core` extended with the 17 names in three tiers (10 RepoRead, 6 RepoWrite, 1 admin) exactly as C1 pins; route-alias names for the object routes follow the existing convention (bearer.rs) and never appear in PROCEDURES.

**Q5 — Registration surfaces.** What does "registration completeness" cover?

[Answer]: Three registrations — `PROCEDURES` list entries (the `system.manifest` advertisement source), `proc_classes` entries, and `versioning.toml` component + scope mapping so `vcs.*`-scoped commits bump correctly; `make rpc-gen` regenerates `packages/api-client` from the hand-maintained `rpc_gen.rs` template (a TS string literal — not derived from the Rust DTOs), so the client coverage check must assert the 17 names appear in the generated `index.ts` rather than trusting regen-diff parity (C9 provenance).
