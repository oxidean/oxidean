# Rules — u1-vcs-wire-foundation

```yaml
rules:
  - id: BR1.1
    statement: "Every vcs.* request rides the existing RpcRequest envelope — {\"procedure\": \"vcs.<name>\", \"input\": {...}} — and every response serializes through the untagged RpcResponse: {ok:true,data:...} on success, {ok:false,error:{code,message,data?}} on failure."
    category: validation
    applies_to: "all 17 vcs.* procedures"
    trigger: "any vcs.* call on POST /api/rpc or /api/rpc/ws"
    logic: "IF a call carries a registered vcs.* procedure THEN it decodes input per that procedure's DTO and returns the untagged envelope; malformed input → rpc.bad_input (shared code, not vcs.*)."
    violation: "rpc.bad_input envelope; never a bare HTTP error on the RPC path"
    source: "AC8.1.1, AC8.1.4"

  - id: BR1.2
    statement: "The procedure set is exactly the 17 FR10.1 names — no more, none invented; all are snake_case under the vcs. prefix. The two object-route alias names (vcs.object_get, vcs.object_put) are the sole, deliberate exception: they exist in proc_classes for route authorization and must never appear in PROCEDURES or gain dispatch arms."
    category: constraint
    applies_to: "PROCEDURES list + proc_classes + dispatch arms"
    trigger: "registration / system.manifest advertisement / coverage tests"
    logic: "IF a name is not in the pinned 17 and is not one of the two route aliases THEN it must not appear in PROCEDURES or proc_classes; the fail-closed coverage test asserts every vcs.* PROCEDURES entry has a class + dispatch arm, and that the two aliases are classified while absent from PROCEDURES."
    violation: "coverage-test failure at build time"
    source: "AC8.1.1, AC8.2.1, FR10.1, FR10.2, R-17 pin"

  - id: BR1.3
    statement: "proc_classes assigns each procedure exactly one tier: RepoRead for the 10 read procedures, RepoWrite for changeset_create/merge/rebase/ref_update/git_import/git_export, RepoAdminWrite for repo_enable (the repo-flag-toggle variant — no new PatProc variant; the admin label is contract shorthand for RepoAdminWrite); anything unlisted resolves SessionOnly (fail closed)."
    category: authorization
    applies_to: "classify_pat_proc + object-route aliases"
    trigger: "PAT authorization of any vcs.* call (the /api/rpc Bearer path accepts PAT + job tokens today — OAuth access tokens on RPC is a host-level extension, not this unit's)"
    logic: "IF procedure in the read tier → RepoRead; IF in write tier → RepoWrite; IF vcs.repo_enable → RepoAdminWrite; ELSE → SessionOnly. Route aliases classify vcs.object_get → RepoRead and vcs.object_put → RepoWrite; they never appear in PROCEDURES. Denial codes by layer: transport failures are auth.unauthenticated / auth.pat_scope / rpc.unknown_procedure; vcs.unauthenticated/vcs.forbidden are emitted by the domain layer and the /api/vcs/* routes."
    violation: "class denial on insufficient scope; the /api/vcs/* routes emit vcs.unauthenticated on missing/cookie-only credentials"
    source: "AC8.6.1, AC8.5.1, R-17 pin"

  - id: BR1.4
    statement: "owner and name travel as two separate string fields on every input — the {\"repo\":\"o/n\"} composite form never exists on this wire. Decode requires both fields; the FG repo-grant layer resolves {owner,name} when present (an absent pair is a decode error, and the MCP grant check delegates required-field validation to the handler — the fail-closed property lives at decode, not the grant)."
    category: constraint
    applies_to: "all procedure inputs; MCP tool args mirror it"
    trigger: "input decode + MCP grant check"
    logic: "IF an input lacks owner or name THEN decode fails rpc.bad_input; repo_target also accepts the {owner,repo}/repository_id aliases the host already maps — vcs.* uses only {owner,name}."
    violation: "rpc.bad_input"
    source: "AC8.1.1, wire convention"

  - id: BR1.5
    statement: "Optional fields serialize absent-not-null — except ref_update's expected_old/new, which are REQUIRED-nullable and encoded as a Nullable<ChangesetId> newtype (enum { Null, Value(ChangesetId) }, custom Deserialize: `null` → Null, value → Value). Because the field is not Option, an absent key raises missing_field — expected_old:null = create-only, new:null = delete, absent = decode error. Plain Option<T> cannot carry this: serde's implicit default maps absent and null to the same None."
    category: constraint
    applies_to: "wire DTO serde behavior"
    trigger: "serialization + ref CAS decode"
    logic: "IF a field is optional THEN it is omitted when unset; expected_old/new are never omitted — absent → rpc.bad_input (missing_field), Null → create-only/delete, Value → CAS target/expectation."
    violation: "CAS semantics break if absent/null conflate — an omitted `new` must not mean delete"
    source: "AC5.1.1, AC5.1.3, contract C1 ref_update pin"

  - id: BR1.6
    statement: "Additive evolution only — consumers ignore unknown fields (no deny_unknown_fields anywhere on the wire); a breaking wire change requires a !-scoped SemVer commit and the wire_version bump applied at both pin sites (oxidean-core's WIRE_VERSION const and the rpc_gen.rs emitted literal)."
    category: policy
    applies_to: "all wire DTOs + generated client"
    trigger: "any field addition/removal across versions"
    logic: "IF a field is added THEN old clients keep working (ignored on decode); IF a breaking change is proposed THEN it rides the SemVer-! path and both WIRE_VERSION sites move together."
    violation: "rpc-sync/compat check failure"
    source: "C9 provenance + wire_compatible semantics"

  - id: BR1.7
    statement: "vcs.* activity-kind constants are the single source for repo_activity push_type values — one constant per mutating procedure, shared by the u2 insert signature, the u7 recorder, and the u10 renderer allow-list."
    category: constraint
    applies_to: "activity feed write + read paths"
    trigger: "any mutating vcs.* op completing"
    logic: "IF an op has no ActivityKind constant THEN no feed row may be written for it; VALID_PUSH_TYPES (oxidean-api) extends from these constants, not a parallel list."
    violation: "feed silently drops ops or renders cs_ ids as commit links"
    source: "AC2.7.1, C4 row contract"

  - id: BR1.8
    statement: "Registration completeness covers the six AC8.2.1/AC8.2.2 surfaces: PROCEDURES entry (oxidean-api, feeds system.manifest) + proc_classes entry + dispatch arm + oxidean-core wire DTOs for every procedure + generated-client coverage + versioning.toml component registration. rpc-sync only proves the committed client matches the hand-maintained rpc_gen.rs template — coverage needs an explicit check (e.g. a test asserting all 17 names appear in the generated index.ts), not regen-diff alone."
    category: validation
    applies_to: "registration surfaces per AC8.2.1/AC8.2.2"
    trigger: "rpc-sync parity + coverage tests + versioning.toml load"
    logic: "IF any surface lacks a procedure THEN the structural check fails; versioning.toml gains [[component]] id=oxidean-vcs kind=crate manifest_path=crates/oxidean-vcs/Cargo.toml tag_prefix=oxidean-vcs-v changelog_path=crates/oxidean-vcs/CHANGELOG.md globs=[crates/oxidean-vcs/**] scopes=[vcs] and depends_on=[] (edges land in u7/u8 with the real Cargo deps — premature edges would cascade vcs bumps into api/ox release plans). U1 also mints the crates/oxidean-vcs skeleton (literal [package] version, src/lib.rs, CHANGELOG.md, workspace member, zero dependencies) since version-check requires a real member at registration. Sequencing pin: u1 lands the PROCEDURES entries together with stub dispatch arms returning vcs.disabled (no repo can be enabled before u4/u7, so the stub is behaviorally correct); u7 replaces the stubs with real handlers — procedures_list_matches_dispatch stays green at every commit boundary. FR15.4's second clause (public surface cites decision IDs in docs/ARCHITECTURE.md) is owned by u7's api-surface docs pass, not this unit."
    violation: "build-time coverage/parity failure — never a runtime surprise"
    source: "AC8.2.1, AC8.2.2, FR10.2, FR15.4"
```

## Rules summary

| id | category | covers |
|----|----------|--------|
| BR1.1 | validation | envelope shape + rpc.bad_input floor |
| BR1.2 | constraint | exact 17-name set, no invention |
| BR1.3 | authorization | three-tier classification + SessionOnly fail-close |
| BR1.4 | constraint | `{owner, name}` two-field convention |
| BR1.5 | constraint | absent-vs-null discipline + ref CAS nulls |
| BR1.6 | policy | additive-only wire evolution |
| BR1.7 | constraint | ActivityKind single-source for feed rows |
| BR1.8 | validation | six-surface registration parity (incl. core wire DTOs) |
