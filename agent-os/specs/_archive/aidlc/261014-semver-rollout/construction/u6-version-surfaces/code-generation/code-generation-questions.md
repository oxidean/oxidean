# U6 Version Surface Wiring — Code Generation Questions

## Plan Approval

Approve this exact Code Generation plan?

[Approval Fingerprint]: sha256:v3:ccc885842d4d0ebf6a13b4b9aad270c968671acdf031933a32186ee1dd5e6ade
[Planned Source]: 79c8e6cdcae5a1f527e114e1670eba7c11dde612671025320dea22a50b092a7d

Options:
- Approve Plan — proceed to code generation
- Request Changes — revise the plan

[Answer]: Approve Plan

## Consolidated Summary Confirmation

The plan covers: `oxidean-core` gains `MCP_PROTOCOL_VERSIONS`,
`wire_compatible()` (caret SemVer predicate), and `ManifestResponse.wire_version`;
the RPC server dual-accepts legacy header `"1"` during the 0.x window and
SemVer headers via the shared predicate; the CLI sends `WIRE_VERSION` and
treats manifests without `wire_version` as legacy 0.1.x (advisory only);
`/cli/latest` reads `latest.json` (absent/corrupt → `version: null,
targets: []`, never the API crate version); OpenAPI `info.version` renders
`WIRE_VERSION` and `docs/openapi.yaml` regenerates; `rpc_gen.rs` emits
`WIRE_VERSION`/`MCP_PROTOCOL_VERSIONS`/`wireCompatible()` into
`packages/api-client`; the five MCP pin sites collapse to the core constant;
`OXIDEAN_RPC_VERSION` is deleted from config and docs. Tests cover the
predicate table, the dual-accept header matrix, `latest()` missing/corrupt/
present, manifest BR6.4 fallback paths, and the generated-client surface.

Options:
- Looks correct — proceed
- Request changes — amend the summary

[Answer]: Looks correct
