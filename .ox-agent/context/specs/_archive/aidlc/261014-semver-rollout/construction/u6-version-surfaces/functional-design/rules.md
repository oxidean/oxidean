# U6 Version Surface Wiring — Business Rules

Source of truth: the fenced YAML block.

```yaml
rules:
  - id: BR6.1
    statement: "Wire compatibility uses caret semantics: while wire <1.0.0, compatible means same `0.MINOR` (any patch); at >=1.0.0, same `MAJOR`. One shared predicate in oxidean-core — the server header check and the generated client both call it."
    category: validation
    applies_to: WireVersion.compat_predicate
    trigger: "every Oxidean-RPC-Version check; every generated-client pre-request check"
    logic: "compatible(a,b): parse semver; if a<1.0.0 and b<1.0.0 → a.MINOR==b.MINOR; else → a.MAJOR==b.MAJOR. Unparseable → incompatible."
    violation: "reject/mismatch"
    source: FR6.1, FR6.6, ADR-011
  - id: BR6.2
    statement: "Transition dual-accept: the server accepts header value `\"1\"` (legacy integer contract = wire 0.1.x semantics) AND caret-compatible wire strings; everything else rejects. Removal condition for the `\"1\"` path: wire >=1.0.0."
    category: policy
    applies_to: server Oxidean-RPC-Version gate
    trigger: "every request during the 0.x window"
    logic: "IF value==\"1\" → treat as wire 0.1.x contract (compatible while server wire is 0.1.x); ELSE parse semver and apply BR6.1. Consequence (deliberate): the first wire bump to 0.2.x hard-rejects all legacy `\"1\"` CLIs — the alias is a 0.1.x contract identifier, not a permanent bypass."
    violation: "reject with wire-mismatch error"
    source: FR6.1 (transition clause), C6
  - id: BR6.3
    statement: "system.manifest co-emits: server_version (oxidean-api CARGO_PKG_VERSION), wire_version (WIRE_VERSION), min_cli_version (MIN_CLI_VERSION floor — ordinal, NOT the compat predicate), protocol_version (legacy alias = 1 while wire <1.0.0)."
    category: policy
    applies_to: ManifestSurface
    trigger: "every system.manifest call"
    logic: "emit all four fields; protocol_version stays `1` until wire reaches 1.x, then the field is removed per docs. The generated client enforces min_cli_version as a HARD floor: own_version < min_cli_version → refuse (unchanged existing semantics); wire compat by contrast stays advisory (BR6.4)."
    violation: n/a
    source: FR6.2, FR6.6
  - id: BR6.4
    statement: "A generated client reading a manifest with no wire_version treats integer protocol_version=1 as the legacy contract (wire 0.1.x semantics); the pre-request compat check stays advisory (warn, don't refuse)."
    category: policy
    applies_to: generated api-client
    trigger: "client-side manifest read"
    logic: "IF wire_version absent AND protocol_version==1 → evaluate compat as if wire_version=\"0.1.x\"; absent BOTH → warn-and-continue advisory."
    violation: "advisory warning only — never a hard refusal (deployed-server compatibility)"
    source: FR6.1, C6
  - id: BR6.5
    statement: "`/cli/latest` serves exclusively from latest.json in OXIDEAN_CLI_DIST_DIR: absent/empty file → the C5 missing-file value (`version: null, targets: []`); never CARGO_PKG_VERSION."
    category: policy
    applies_to: CliLatestResponse
    trigger: "every /cli/latest request"
    logic: "read latest.json → emit {version, targets}; missing/corrupt → missing-file default. The endpoint reflects dist state, not server version."
    violation: n/a
    source: FR6.3, C5
  - id: BR6.6
    statement: "OpenAPI info.version renders WIRE_VERSION (replacing hardcoded `\"1\"` in rest/spec.rs); regenerated docs/openapi.yaml follows automatically."
    category: policy
    applies_to: OpenAPI spec surface
    trigger: "spec.rs change + openapi-gen run"
    logic: "info.version = WIRE_VERSION literal; generated yaml drift is caught by existing sync checks."
    violation: n/a
    source: FR6.4
  - id: BR6.7
    statement: "All five MCP pin sites collapse to canonical `MCP_PROTOCOL_VERSIONS` in oxidean-core; the TS copy is rpc-gen output — hand-editing the generated constant is forbidden (same drift rule as the rest of api-client)."
    category: constraint
    applies_to: McpPin
    trigger: "rpc-gen regeneration"
    logic: "five current sites (server handler, client init, spec constant, TS client, tests) read from the one constant after codegen."
    violation: n/a
    source: FR6.4
  - id: BR6.8
    statement: "`OXIDEAN_RPC_VERSION` env/config is deleted outright — it is already inert (dead config), so removal is cleanup, not a deprecation cycle."
    category: constraint
    applies_to: config surface
    trigger: "implementation"
    logic: "remove the field, its parsing, and its docs reference in one pass."
    violation: n/a
    source: FR6.5
```

## Rules summary

| Rule | Category | In one line |
|---|---|---|
| BR6.1 | validation | caret compat: 0.x same minor, ≥1 same major |
| BR6.2 | policy | server dual-accepts `"1"` during 0.x window |
| BR6.3 | policy | manifest co-emits 4 fields incl. legacy alias |
| BR6.4 | policy | absent wire_version → legacy contract, advisory only |
| BR6.5 | policy | /cli/latest reads latest.json, never crate version |
| BR6.6 | policy | OpenAPI info.version = WIRE_VERSION |
| BR6.7 | constraint | one MCP_PROTOCOL_VERSIONS via rpc-gen |
| BR6.8 | constraint | dead OXIDEAN_RPC_VERSION deleted |
