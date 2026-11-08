# Security Design — u1-vcs-wire-foundation

This unit adds no runtime paths of its own — its security design is the *shape* the wire layer forces on every later surface. Each SR from nfr-requirements maps to a concrete mechanism.

## SD1 — Fail-closed classification (SR1)

**Mechanism.** `proc_classes.rs` gains one contiguous `vcs.*` match block: 17 procedure arms + 2 object-route alias arms, each returning its pinned tier (`RepoRead`/`RepoWrite`/`RepoAdminWrite`). The existing `_ => SessionOnly` fallthrough is untouched — any `vcs.*` name absent from the block stays denied to token callers with no additional code.

**Evidence mechanism.** The coverage test is the enforcement: it iterates the pinned 19-name set and asserts (a) each PROCEDURES-listed name has a non-`SessionOnly` class, (b) each has a dispatch arm, (c) the two aliases are classified *and* absent from `PROCEDURES`. Adding a `vcs.*` name without a tier entry fails the build — the design never relies on remembering to classify.

**Why not a data table.** `classify_pat_proc`'s match body is ~240 lines of grouped literal arms (proc_classes.rs is 639 total); a `const` table would introduce a second style for zero gain — the test, not the encoding, is the safety property.

## SD2 — Denial-code layering (SR2)

**Mechanism.** Layering is structural, not a runtime check: the closed `VcsErrorCode` enum contains only domain codes (`vcs.unauthenticated`, `vcs.forbidden` included) and converts into `AppError.code` only inside domain handlers / `/api/vcs/*` route authz. The transport layer (cookie/PAT/job-token resolution, `Oxidean-RPC-Version` gate, procedure-name resolution, PAT scope check) emits its existing `auth.*`/`rpc.*` codes and never touches `VcsErrorCode`. The two layers can't cross-contaminate because there is no code path from transport code into the enum.

**Route-level authz shape (u7 builds it; u1 pins the contract).** `/api/vcs/*` object routes resolve PAT Basic/Bearer or OAuth access tokens — cookies never authorize — and deny with `vcs.unauthenticated` (missing/cookie-only credential) or `vcs.forbidden` (resolved but insufficient tier, or FG-uncovered repo → `vcs.not_found` for anti-enumeration symmetry).

**Client contract on `system.manifest`.** U1's stub dispatch arms mean `procedures` advertises all 17 names before handlers exist — pinned semantics: `procedures` = names dispatch recognizes, not enabled features. Capability discovery is `vcs.status` (u7); a `capabilities.vcs` flag is additive if needed. The manifest invariant tests (`procedures_list_matches_dispatch`, `manifest_shape`) require the names to appear — suppressing them would break the parity invariant.

## SD3 — Bounded error payloads (SR3)

**Mechanism.** `AppError.data` is typed JSON produced by `VcsErrorCode::with_data` helpers — the payload shapes are pinned per code (e.g. `ref_conflict` → `{actual: changeset_id}`, `conflict` → `{conflicts: [{path, kind, detail?}…≤500], truncated: bool}`). The 500 cap and `truncated` flag are serializer-level bounds: the constructor truncates rather than letting the caller mint an unbounded body. No redaction pass — every field is an identifier the caller is already authorized to see (authz precedes payload construction).

## SD4 — Decode safety (SR4)

**Mechanism.** Three cooperating decode rules:
- `{owner, name}` are plain required `String` fields — serde `missing_field` on absence (post-authz; FG tokens fail earlier at `repo_target` with `auth.pat_scope`).
- `ref_update`'s CAS fields use `Nullable<ChangesetId>` — a non-`Option` field so absence raises `missing_field` while explicit `null` maps to the `Null` variant (create-only/delete) and a value maps to `Value(id)` (CAS). Two implementation gotchas pinned: derived `Serialize` on the enum would emit tagged `{"Null":…}` — it needs a custom `Serialize` (or `untagged`) emitting bare `null`/`value`; and derived `JsonSchema` would misdescribe the wire shape — a manual impl emits `["null", <T-schema>]`, or the lighter field-level `#[schemars(with = "Option<ChangesetId>")]` if the impl proves noisy.
- No `deny_unknown_fields` anywhere — additive evolution depends on consumers ignoring unknown fields.

**Type-layer vs handler-layer split.** DTO fields are unvalidated serde structs by design — string lengths, `limit ≤500`, `changes ≤10_000`, `validate_repo_name`-equivalent checks all land in u7's handlers (contract C1 `limits`, `vcs.too_large`). The wire types pin the *shape*; the handlers pin the *bounds*.

## SD5 — No secrets on the wire (SR5)

**Mechanism.** By construction: no wire field accepts credential material (no token/password/secret fields in any of the 17 input shapes); object bytes ride `/api/vcs/*` routes or the ≤256 KiB `content_b64` inline cap — both already bounded by contract. `vcs_types.rs` is inspectable in one pass precisely so this property stays auditable.
