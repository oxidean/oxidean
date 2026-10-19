# Entities — u1-vcs-wire-foundation

Wire-foundation unit: the "entities" are the portable data shapes every `vcs.*` surface shares — request/response payloads, the error taxonomy, identifier conventions, and activity-kind constants. All live in `oxidean-core` (ADR-002); domain-internal types in `oxidean-vcs` must never leak onto the wire.

```yaml
entities:
  - name: VcsProcedureInput
    description: "Common input prefix every vcs.* procedure carries; merges into each procedure's input object"
    attributes:
      - {name: owner, type: string, required: true, description: "repo owner slug"}
      - {name: name, type: string, required: true, description: "repo name slug — owner/name are separate fields (never {\"repo\":\"o/n\"})"}
    constraints: ["both fields required on all 17 procedures"]
    relationships: "1 per procedure call → resolves to repo id server-side"

  - name: VcsErrorCode
    description: "Closed string enum of wire-stable error codes (AC8.1.4) — per-variant serde rename to the dotted code (rename_all=snake_case would emit vcs_disabled, not vcs.disabled); converts into AppError.code:String at AppError::new/with_data"
    attributes:
      - name: code
        type: "enum string"
        required: true
        allowed_values: [vcs.disabled, vcs.not_found, vcs.invalid_input, vcs.ref_conflict, vcs.conflict, vcs.no_common_ancestor, vcs.quota_exceeded, vcs.too_large, vcs.import_state, vcs.unauthenticated, vcs.forbidden]
        description: "exhaustive vcs.* set — these codes are emitted by the domain layer and the /api/vcs/* routes; transport-layer denials keep platform codes (auth.unauthenticated, auth.pat_scope, rpc.unknown_procedure, repo.not_found anti-enumeration); shared codes (repo.archived, db.not_configured, rpc.bad_input) come from the platform taxonomy"
    relationships: "carried by AppError.code; typed payload lands in AppError.data"

  - name: ErrorData
    description: "Typed payloads riding AppError.data for the codes that carry detail"
    attributes:
      - {name: ref_conflict.actual, type: "changeset_id|null", required: false, description: "stored ref value the CAS expected (R-19 pin)"}
      - {name: conflict.conflicts, type: "[{path: string, kind: string, detail: string?}]", required: false, description: "typed merge/rebase denial list (R-05); kind values per U5's conflict taxonomy (binary/mode/type/rename) — open string on the wire so a new kind is additive"}
      - {name: forbidden.verdicts, type: "ProtectionBlockReasons", required: false, description: "data = { verdicts: ProtectionBlockReasons } — the existing struct (protection_types.rs:266-279) nested under the `verdicts` key per C1; the verdict computation path is identical to a git push denial, only the envelope adds the key (AC7.2.2)"}

  - name: ChangesetId
    description: "Opaque server-minted changeset identifier (cs_-prefixed) — never a git oid"
    attributes:
      - {name: value, type: string, required: true, constraints: "format cs_<ulid>; compared by equality only"}

  - name: RefName
    description: "Wire field naming a named head — serialized field name is `ref` (ADR-009); the Rust field follows the house pattern `pub ref_name: String` + serde rename (13 existing uses in repo_types.rs) — values like `main` or `agent/<task>`"
    attributes:
      - {name: ref_name, type: string, required: true, description: "serializes as `ref`"}

  - name: ManifestEntry
    description: "One changed-file row inside changeset_get/log manifests and changeset_create inputs"
    attributes:
      - {name: path, type: string, required: true}
      - {name: kind, type: "enum string", required: true, allowed_values: [add, modify, delete, rename]}
      - {name: mode, type: string, required: true, allowed_values: ["100644", "100755", "120000"], description: "octal mode as string"}
      - {name: old_path, type: "string?", required: false, description: "source path when kind=rename — preserves rename round-trip (R-13)"}
      - {name: blob, type: "sha256 string?", required: false, description: "existing object reference"}
      - {name: content_b64, type: "string?", required: false, description: "inline content transport ≤256 KiB (AC2.1.2)"}
    constraints: ["exactly one of blob/content_b64 per non-delete entry; delete carries neither (R-29)"]

  - name: PageArgs / PageResult
    description: "Uniform pagination shape across log/tree/diff/blame/path_history/ref_list — carried as per-DTO limit/cursor fields (not shared types); deliberately diverges from the legacy offset+limit convention since the VCS surfaces are greenfield"
    attributes:
      - {name: limit, type: "int?", required: false, constraints: "≤500 entries/call"}
      - {name: cursor, type: "string?", required: false, description: "opaque; absent in response = end"}

  - name: RefUpdateCasFields
    description: "vcs.ref_update's required-nullable CAS pair — encoded as a `Nullable<ChangesetId>` newtype (enum { Null, Value(ChangesetId) } with custom Deserialize accepting `null` → Null and a value → Value). Because the field type is not Option, serde raises missing_field on an absent key: absent → rpc.bad_input, null → Null (create-only/delete), value → Value (CAS). serde's implicit None-default on Option<T> collapses absent/null — Option cannot provide this encoding"
    attributes:
      - {name: expected_old, type: "Nullable<changeset_id>", required: true, description: "Null = create-only"}
      - {name: new, type: "Nullable<changeset_id>", required: true, description: "Null = delete"}
    constraints: ["both fields must be present in the input — absence is rpc.bad_input (missing_field), not a default"]

  - name: VcsStatusResponse
    description: "vcs.status response (frozen by contract design after its provisional state)"
    attributes:
      - {name: enabled, type: boolean, required: true}
      - {name: heads, type: "[{name, changeset_id}]", required: true}
      - {name: changeset_count, type: int, required: true}
      - {name: object_count, type: int, required: true}
      - {name: storage_bytes, type: int, required: true}

  - name: VcsRefPublic
    description: "vcs.ref_list entry (house naming: Vcs{Op}Request/Vcs{Op}Response/Vcs{Thing}Public). A listed ref always resolves to a changeset — non-null per the C1 ref_list freeze. ref_update's result is a separate VcsRefUpdateResponse whose changeset_id IS nullable (a deleted ref resolves to null)"
    attributes:
      - {name: name, type: string, required: true}
      - {name: changeset_id, type: "changeset_id", required: true, description: "non-null — the freeze; nullability lives only on VcsRefUpdateResponse"}
      - {name: updated_at, type: "timestamp", required: true, description: "unconditional per the C1 ref_list output"}

  - name: VcsRefUpdateResponse
    description: "vcs.ref_update result — distinct from VcsRefPublic because a delete makes the target null"
    attributes:
      - {name: ref, type: string, required: true}
      - {name: changeset_id, type: "changeset_id|null", required: true, description: "null after a delete (C1 ref_update output pin)"}

  - name: ActivityKind
    description: "Shared vcs.* activity-kind constants (push_type values on repo_activity rows) — one per mutating procedure per C4"
    attributes:
      - {name: kind, type: "enum string", required: true, allowed_values: ["vcs.repo_enable", "vcs.changeset_create", "vcs.merge", "vcs.rebase", "vcs.ref_update", "vcs.git_import", "vcs.git_export"]}
    relationships: "consumed by u2 insert signature + u7 recorder + u10 renderer"

  - name: RepoActivityItem (existing, widened)
    description: "The C4 nullability decision ripples onto the wire DTO (repo_types.rs:521-537): ref_name/before/after/commits_count become Option (plain Option, no skip_serializing_if — renders explicit null). ref_short (derived from ref_name at render, activity.rs:33-39) becomes Option too and renders null when ref_name is null. Ownership split (nfr-design LC3): u1 lands the DTO widening + row_to_item Some() wrap + apps/web reader guards; u2 lands the InsertRepoActivity signature + column nullability + RepoActivityRow widening — and removes the Some() wrap in oxidean-api's repo/activity.rs when Row fields widen (crate-boundary exception). Additive-safe for old payloads, breaking for generated-client field types — rides the versioning bump."
    attributes:
      - {name: ref_name, type: "string → string|null", required: false}
      - {name: ref_short, type: "string → string|null", required: false, description: "derived; null iff ref_name is null"}
      - {name: before, type: "string → string|null", required: false, description: "wire field is `before`; `before_oid` is the DB column/façade arg name only"}
      - {name: after, type: "string → string|null", required: false, description: "wire field is `after`; `after_oid` is the DB column/façade arg name only"}
      - {name: commits_count, type: "int → int|null", required: false}
    constraints: ["row mapper must read NULL columns without erroring (u2 layer); existing push rows always populate them"]

procedure-dtos:
  description: "The 17 per-procedure request/response pairs follow house naming Vcs{Op}Request / Vcs{Op}Response in crates/oxidean-core/src/vcs_types.rs (new module, re-exported via lib.rs pub use); shapes per contract C1"
  list: [VcsRepoEnable, VcsStatus, VcsChangesetCreate, VcsChangesetGet, VcsLog, VcsDiff, VcsTree, VcsFileGet, VcsBlame, VcsPathHistory, VcsMergeBase, VcsMerge, VcsRebase, VcsRefList, VcsRefUpdate, VcsGitImport, VcsGitExport]
```

## Summary

Thirteen wire entities: the shared `VcsProcedureInput` `{owner, name}` prefix, the closed `VcsErrorCode` taxonomy (11 codes) plus its typed `ErrorData` payloads, the two identifier shapes (`ChangesetId`, `RefName`/`ref_name`), the `ManifestEntry` file-change row (with `old_path` and the two-transport blob fields), the uniform limit/cursor pagination pair, `RefUpdateCasFields` (the `Nullable<T>` encoding), `VcsStatusResponse`, `VcsRefPublic`, `VcsRefUpdateResponse` (nullable delete result), the `ActivityKind` constants feeding u2/u7/u10, and the `RepoActivityItem` widening owned jointly with u2. Everything downstream (u7 dispatch, u8 CLI output, u9 MCP descriptors, the generated TS client) consumes these shapes — a change here is a wire change and follows the breaking-change rules in contract-ownership.
