# Tech Stack Decisions — u1-vcs-wire-foundation

All selections are existing-stack affirmations — this unit adds zero dependencies.

| Decision | Choice | Rationale |
|---|---|---|
| Wire language | Rust `struct`/`enum` in `crates/oxidean-core/src/vcs_types.rs`, re-exported via `lib.rs` `pub use` | Single definition site shared by server (oxidean-api), CLI (ox), and MCP surfaces; house convention for RPC DTOs |
| (De)serialization | `serde` + `serde_json` | Only serialization stack in `oxidean-core`; `#[serde(rename = "ref")]` house pattern for the `ref` field (13 existing uses in `repo_types.rs`; 15 across `crates/`) |
| Schema derivation | `schemars::JsonSchema` on every wire DTO | Existing convention (`repo_types.rs`, `protection_types.rs`); keeps generated docs/diagnostics honest |
| Error taxonomy | `VcsErrorCode` enum, per-variant `#[serde(rename = "vcs.<code>")]`, plus an explicit `impl From<VcsErrorCode> for String` (serde rename yields `Serialize`, not `Into<String>`) for `AppError::new(code)`/`with_data` | Container-level snake_case cannot emit dotted codes; enum gives exhaustiveness checks on typed `data` payloads |
| Absent-vs-null fields | `Nullable<T>` newtype (`enum { Null, Value(T) }` + custom `Deserialize`) | serde implicitly defaults `Option<T>` on absent — only a non-`Option` field raises `missing_field`; no `serde_with` dep exists to reuse |
| Envelope | existing untagged `RpcResponse` + `AppError{code,message,data?}` | Contract freeze C1; no new envelope machinery |
| Client generation | `make rpc-gen` → hand-maintained `rpc_gen.rs` template in oxidean-api + `check-rpc-sync.sh` parity + a generated-coverage test asserting the 17 names in `index.ts` | The template is the source; regen-diff alone proves nothing about coverage |
| Versioning | `versioning.toml` `[[component]]` `oxidean-vcs` (crate) | `make version-check` enforces bidirectional registry↔workspace completeness: a registered `manifest_path` must be a real workspace member with a **literal** `[package] version` (`version.workspace = true` is rejected by the checker). U1 therefore also mints the crate skeleton — `crates/oxidean-vcs/{Cargo.toml,src/lib.rs,CHANGELOG.md}` + workspace membership — as an empty shell the later units fill. The `depends_on` edges from `oxidean-api`/`ox` are **not** registered in U1: they must mirror real Cargo dependencies, and U1's stub dispatch arms return `vcs.disabled` without one; each edge lands in the unit that adds the real `Cargo.toml` dep (u7 api, u8 ox). |
| Pagination | per-DTO `limit`/`cursor` fields | Deliberate divergence from legacy `offset`+`limit`; cursor is opaque and stable across inserts |
| Identifiers | `cs_<ulid>` strings on the wire; `sha256:`-prefixed hex for objects | Opaque to clients; minting is u4's concern — the wire type is a validated string |
