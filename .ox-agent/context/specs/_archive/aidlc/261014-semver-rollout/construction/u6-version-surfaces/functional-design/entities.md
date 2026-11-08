# U6 Version Surface Wiring — Entities

Source of truth: the fenced YAML block.

```yaml
entities:
  - name: WireVersion
    description: The wire-contract version — a literal SemVer constant `WIRE_VERSION` in `oxidean-core`, owned by the `wire` registry entry (kind=contract).
    attributes:
      - {name: value, type: semver, required: true, constraint: "literal in crates/oxidean-core; bumped only via the versioning tool (ManifestEdit field=wire_constant)"}
      - {name: compat_predicate, type: rule, required: true, constraint: "<1.0.0 → same 0.MINOR any patch; >=1.0.0 → same MAJOR"}
      - {name: legacy_alias, type: integer, required: true, constraint: "emits `1` while wire <1.0.0 — the integer contract deployed CLIs already speak"}
    relationships:
      - {to: ManifestSurface, kind: "1..1", direction: "reported by"}
  - name: ManifestSurface
    description: `system.manifest` RPC response — the version-reporting surface external consumers read.
    attributes:
      - {name: server_version, type: semver, required: true, source: "oxidean-api CARGO_PKG_VERSION"}
      - {name: wire_version, type: semver, required: true, source: WIRE_VERSION}
      - {name: min_cli_version, type: semver, required: true, source: MIN_CLI_VERSION — distinct ordinal floor}
      - {name: protocol_version, type: integer, required: true, constraint: "legacy alias emitting `1` while wire <1.0.0; documented removal condition: wire >=1.0.0"}
    relationships: []
  - name: DistManifest
    description: `latest.json` — the dist manifest the `ox-v*` stable release flow writes and `/cli/latest` reads (C5).
    attributes:
      - {name: version, type: semver, required: true, constraint: "the ox CLI version this dist was cut from"}
      - {name: targets, type: "{target, sha256, bytes}[]", required: true}
      - {name: dist_commit, type: sha40, required: true, constraint: "commit the payload was built from — not the tag sha"}
    constraints:
      - "prerelease tags NEVER write it (C4/C5 stable lane only); absent file → documented missing-file default response"
    relationships:
      - {to: CliLatestResponse, kind: "1..1", direction: "drives"}
  - name: CliLatestResponse
    description: `/cli/latest` JSON response.
    attributes:
      - {name: version, type: "semver|null", required: true, constraint: "null + empty targets when latest.json absent (C5 missing-file value)"}
      - {name: targets, type: "array", required: true}
    constraints:
      - "never sourced from oxidean-api CARGO_PKG_VERSION — the current behaviour is the bug being fixed"
  - name: McpPin
    description: MCP protocol/spec revision pins — deduplicated to one canonical constant.
    attributes:
      - {name: mcp_protocol_versions, type: "string[]", required: true, constraint: "canonical const MCP_PROTOCOL_VERSIONS in oxidean-core; propagated to @oxidean/api-client via rpc-gen — never hand-maintained in TS"}
    constraints:
      - "the five duplicate pin sites collapse to this one constant (FR6.4)"
```

## Entity summary

`WireVersion` (the contract constant + compat rule + legacy integer alias), `ManifestSurface` (the four emitted fields), `DistManifest`/`CliLatestResponse` (latest.json→`/cli/latest`), `McpPin` (deduplicated pins). Dead surface removed outright: `OXIDEAN_RPC_VERSION` env/config (already inert).
