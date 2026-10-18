# Code Generation Plan — u1-vcs-wire-foundation

Implements the wire foundation per functional-design/nfr-design artifacts. Test strategy: standard (unit tests per component + boundary assertions). All shapes pinned by contract-summary C1 + entities.md; mechanism pins from nfr-design LC1–LC6.

## Steps

### 1. `crates/oxidean-core/src/vcs_types.rs` (new module — LC1)
- [ ] Wire helpers: `pub enum Nullable<T>` (`Null`/`Value(T)`, custom Deserialize, untagged-style Serialize emitting bare null/value, `is_null()`/`value()`; field-level `#[schemars(with = "Option<T>")]` on CAS fields); `ChangesetId` validated newtype (`cs_<ulid>` shape); `ObjectId` (`sha256:`-prefixed hex). *(AC8.1.x)*
- [ ] `VcsErrorCode`: closed enum, per-variant `#[serde(rename = "vcs.<code>")]` — the 11 codes (disabled, not_found, invalid_input, ref_conflict, conflict, no_common_ancestor, quota_exceeded, too_large, import_state, unauthenticated, forbidden); `as_str()`; `impl From<VcsErrorCode> for String`. *(AC8.1.4)*
- [ ] Shared shapes: `VcsChangesetPublic`, `VcsRefPublic`, `ManifestEntry`, `ErrorData`/typed error payloads, `VcsHeadPublic`, pagination fields (`limit`, `cursor` — per-DTO, `Option` + `skip_serializing_if`). *(AC8.1.1–8.1.3)*
- [ ] 17 `Vcs{Op}Request`/`Vcs{Op}Response` pairs — exact C1 signatures: repo_enable `{owner,name,enabled}→{enabled}`; status `{owner,name}→{enabled,heads,changeset_count,object_count,storage_bytes}`; changeset_create `{owner,name,parents,message,author?,changes:[ManifestEntry]}→{changeset_id,manifest_hash}`; changeset_get `{owner,name,id}→{changeset_id,parents,author,message,committed_at,manifest}`; log `{owner,name,from,limit?,cursor?}→{entries,cursor?}`; tree `{owner,name,changeset_id,path_prefix?,limit?,cursor?}→{entries:[{path,mode,blob}],cursor?}`; diff `{owner,name,from,to,paths?,limit?,cursor?}→{entries:[{path,kind,old_mode?,new_mode?,old_blob?,new_blob?,patch?}],cursor?,totals}`; blame `{owner,name,path,at?,limit?,cursor?}→{entries:[{changeset_id,author,content}],cursor?}`; path_history `{owner,name,path,limit?,cursor?}→{entries:[{changeset_id,kind,path,committed_at}],cursor?}`; merge_base `{owner,name,a,b}→{merge_base}`; file_get `{owner,name,changeset_id,path}→{blob,mode,size,content_b64|null,too_large}`; merge `{owner,name,a,b,message?}→{changeset_id,fast_forward?}`; rebase `{owner,name,changeset_id,new_parent_id,message?}→{head}`; ref_list `{owner,name,limit?,cursor?}→{entries:[VcsRefPublic],cursor?}`; ref_update `{owner,name,ref:serde-rename,expected_old:Nullable,new:Nullable}→{ref,changeset_id|null}`; git_import `{owner,name,from?,cursor?,limit?}→{imported,cursor?,done,heads}`; git_export `{owner,name,head,branch,force?}→{branch,git_oid}`. *(AC8.1.1–8.1.3, AC8.2.1)*
- [ ] Seven `vcs.*` activity-kind constants (`VCS_ACTIVITY_*` or `pub const` strings). *(AC2.6.1 share)*
- [ ] Re-export from `lib.rs` (`pub mod vcs_types; pub use vcs_types::*`).

### 2. `proc_classes.rs` `vcs.*` block (LC2)
- [ ] 17 procedure arms + 2 alias arms as literal `"vcs.x" =>` lines: RepoRead ×10, RepoWrite ×6, RepoAdminWrite (`vcs.repo_enable`), aliases `vcs.object_get`→RepoRead, `vcs.object_put`→RepoWrite. Unlisted stays `SessionOnly` fallthrough. *(AC8.6.1)*

### 3. `oxidean-api` registration surfaces (LC5)
- [ ] `PROCEDURES` gains the 17 names; dispatch gains **17 separate single-name `"vcs.x" =>` arms** each returning `AppError` `vcs.disabled` (parity-scanner shape pinned). *(AC8.1.2)*
- [ ] `rpc_gen.rs` template: 17 client methods + generated `index.ts` types; keep `WIRE_VERSION` literal equal to `oxidean_core::WIRE_VERSION`; run `make rpc-gen`, commit output. *(AC8.2.2)*

### 4. `RepoActivityItem` widening (LC3)
- [ ] `repo_types.rs`: five fields → plain `Option` (NO `skip_serializing_if`); `repo/activity.rs` `row_to_item` wraps `Some(...)`; `apps/web` `repo-activity-row.tsrx` null guards (`?? item.ref_name`, `after != null`, `commits_count ?? 0`). *(AC2.6.1 share)*
- [ ] `wire`-scoped breaking commit for the widening (`feat(wire)!:` shape — wire bumps 0.1→0.2; `globs=[]` so scope drives attribution).

### 5. `crates/oxidean-vcs` skeleton + versioning (LC4)
- [ ] `Cargo.toml` (literal `version = "0.1.0"`, workspace-inherited edition/license, zero deps), `src/lib.rs`, `CHANGELOG.md`; root `Cargo.toml` `members`; `versioning.toml` `[[component]]` (`id="oxidean-vcs"`, `kind="crate"`, `manifest_path`, `tag_prefix="oxidean-vcs-v"`, `changelog_path`, `globs=["crates/oxidean-vcs/**"]`, `scopes=["vcs"]`, `depends_on=[]`); `Cargo.lock` member delta in the same tree. *(AC8.2.2)*

### 6. Tests (LC6 — mandatory)
- [ ] proc_classes coverage: all 17 PROCEDURES-listed vcs.* → non-SessionOnly + dispatch arm; aliases classified & absent from PROCEDURES.
- [ ] Serde round-trips: `Nullable` absent→missing_field / null→Null / value→Value; `VcsErrorCode` dotted strings; `ref` field rename; `Option` absent-not-null; widened `RepoActivityItem` emits explicit null.
- [ ] Generated-client coverage: all 17 names in `packages/api-client/index.ts`.
- [ ] `WIRE_VERSION` sync assertion (template literal == const) — interpolate or string-compare.

## Verification
`cargo nextest` on touched crates, `make rpc-sync-check`, `make version-check`, `make web-lint`/`web-format-check`, `make dead-code-check`, `cargo fmt --check`, rustdoc-clean.

## Story traceability
Steps 1–3 → US8.1/8.2/8.6 (+ shared AC1.1.2 wire envelope); step 4 → AC2.6.1/AC5.1.1 shares; step 5 → AC8.2.2; step 6 → NFR5/NFR9/NFR10.
