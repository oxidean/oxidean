# U3 Change Detection & Bump Engine — Entities

Source of truth: the fenced YAML block.

```yaml
entities:
  - name: Attribution
    description: One commit's contribution to one component — the join of parser signals (U2) and path globs (U1).
    attributes:
      - {name: sha, type: sha40, required: true}
      - {name: componentId, type: id, required: true, constraint: "resolves to a RegistryEntry"}
      - {name: type, type: "enum?", required: "iff not exempt", values: "U2 ConventionalCommit.type"}
      - {name: breaking, type: boolean, required: true}
      - {name: source, type: enum, required: true, values: [scope, glob, scope_and_glob]}
      - {name: ambiguous, type: boolean, required: true, constraint: "true when the path matched >1 entry's globs (Q1) — warning emitted"}
    constraints:
      - "exempt commits produce no Attribution (parser short-circuits them)"
    relationships:
      - {to: RegistryEntry, kind: "n..1", direction: "attributes to"}
  - name: DependencyEdge
    description: A registry `depends_on` edge — the propagation path for shared-source changes.
    attributes:
      - {name: from, type: id, required: true}
      - {name: to, type: id, required: true, constraint: "to depends_on from"}
      - {name: transitive, type: boolean, required: true, constraint: "closure membership, not a stored field"}
    relationships:
      - {to: RegistryEntry, kind: "n..m", direction: "DAG"}
  - name: BumpPlan
    description: The release-plan output (C3) — per-entry from→to with the evidence trail.
    attributes:
      - {name: generatedAt, type: iso8601, required: true}
      - {name: baseRef, type: string, required: true, constraint: "commit the plan was computed at"}
      - {name: entries, type: "BumpPlanEntry[]", required: true, constraint: "only components with bump ≠ none or forced note"}
      - {name: channel, type: enum, required: true, values: [stable, alpha, beta, nightly], default: stable}
    relationships:
      - {to: BumpPlanEntry, kind: "1..n", direction: "contains"}
  - name: BumpPlanEntry
    description: One component's planned bump in a plan (C3 `entries[]` row).
    attributes:
      - {name: component, type: id, required: true}
      - {name: fromVersion, type: semver, required: true, constraint: "read from the manifest/constant — never the registry"}
      - {name: toVersion, type: semver, required: true, constraint: "may carry -alpha.N/-beta.N/-nightly.YYYYMMDDN (FR8.3)"}
      - {name: bump, type: enum, required: true, values: [none, patch, minor, major]}
      - {name: direct, type: boolean, required: true}
      - {name: propagatedFrom, type: "id[]", required: true, constraint: "entries whose bumps caused this one (feeds FR4.4)"}
      - {name: commits, type: "{sha,type,scope,breaking,subject}[]", required: true, constraint: "the entry's OWN directly-attributed commits only; empty for propagated-only entries — origins are named in propagatedFrom"}
    relationships:
      - {to: Attribution, kind: "1..n", direction: "summarises"}
  - name: ManifestEdit
    description: One applied version write to a manifest or constant file (FR3.3).
    attributes:
      - {name: path, type: path, required: true}
      - {name: componentId, type: id, required: true}
      - {name: field, type: enum, required: true, values: ["cargo.version", "package.version", "wire_constant"]}
      - {name: before, type: semver, required: true}
      - {name: after, type: semver, required: true}
    constraints:
      - "only in-registry version fields are writable — fixtures, templates, lockfile schema markers never match (FR2.4 boundary, writer side)"
    relationships:
      - {to: RegistryEntry, kind: "n..1", direction: "writes"}
```

## Entity summary

`Attribution` (commit→component join), `DependencyEdge` (propagation path), `BumpPlan`/`BumpPlanEntry` (the C3 output — versions always read from manifests, never the registry), `ManifestEdit` (the discipline record: only in-registry fields are writable).
