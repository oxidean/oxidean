# Unit of Work — Dependency DAG

Topology only — no build-order recommendation or critical-path claim (those are 2.9 economic decisions; delivery-planning is skipped in this scope, so the DAG itself defines what unblocks what).

## Edges (prose)

- **U1 version-registry** depends on nothing — it is the foundation every other unit reads (registry schema, entries, vocabulary).
- **U2 commit-grammar** depends on **U1** — scope aliases, allowed types, and reserved scopes come from the registry.
- **U3 change-detection** depends on **U1** (registry entries, globs, dependency edges, manifest paths) and **U2** (the shared commit parser).
- **U4 changelog-release** depends on **U3** (consumes the release plan) and **U1** (tag prefixes, changelog paths, baseline commit).
- **U5 commit-lint-gates** depends on **U2** (the `commit-check` validator both surfaces call) and **U1** (registry completeness check in the same CI gate).
- **U6 version-surfaces** depends on **U1** (the wire entry's registry identity) and **U4** — `/cli/latest` reads the `latest.json` manifest U4's `ox-v*` release flow writes (ADR-007); the file is introduced by this work so the consumer's reader contract lands after the producer.
- **U7 contributor-guidance** depends on **U2** (documents the grammar), **U5** (documents hook/gate setup — FR7.1's `make setup` coverage), and **U4** (documents the release flow). Ratified divergence: the component model gives ContributorGuidance edges to CommitGrammar+ReleaseOrchestrator only; the U7→U5 edge is a unit-level addition for the hook-install docs.

## Machine-readable edge block

```yaml
units:
  - name: u1-version-registry
    kind: spec
    depends_on: []
  - name: u2-commit-grammar
    kind: library
    depends_on: [u1-version-registry]
  - name: u3-change-detection
    kind: library
    depends_on: [u1-version-registry, u2-commit-grammar]
  - name: u4-changelog-release
    kind: packaging
    depends_on: [u1-version-registry, u3-change-detection]
  - name: u5-commit-lint-gates
    kind: packaging
    depends_on: [u1-version-registry, u2-commit-grammar]
  - name: u6-version-surfaces
    kind: library
    depends_on: [u1-version-registry, u4-changelog-release]
  - name: u7-contributor-guidance
    depends_on: [u2-commit-grammar, u4-changelog-release, u5-commit-lint-gates]
```

## Integration Points

| From → To | Contract |
|---|---|
| U1 → U2, U3, U4, U5, U6 | `versioning.toml` schema — entry ids, aliases, globs, edges, tag prefixes, changelog paths, baseline (formalised in contract-design) |
| U2 → U3, U5 | commit-parser module + `commit-check` subcommand interface (message/range in; valid/signal/exempt out) |
| U3 → U4 | release-plan data shape — per-entry fromVersion→toVersion, bump level, attributed commits, propagatedFrom, prerelease |
| U4 → CI | `*-v*.*.*` tag grammar + workflow trigger contract |
| U4 → U6 | `latest.json` dist-manifest shape — U4's release flow writes it, U6's `/cli/latest` reads it |
| U1+U2+U3 → U5's CI job | `commit-check` (range + title) and registry `check` invoked in one gate |

## Parallel Development Opportunities

- After **U1**: **U2** and **U6** are independent of each other.
- After **U1 + U2**: **U3** and **U5** can proceed in parallel.
- **U4** unblocks once **U3** lands (note U6 also consumes its `latest.json` contract); **U7** waits for U2/U4/U5 to have something to document.
- Valid topological orderings exist with U5/U3/U6 interleaved — no unit is forced to serialize behind U4 except U7.
