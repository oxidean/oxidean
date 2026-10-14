# U1 Version Registry — Entities

Source of truth: the fenced YAML block. Human summary follows.

```yaml
entities:
  - name: RegistryConfig
    description: The whole `versioning.toml` document — metadata for the versioned set. Never carries version numbers (ADR-001).
    attributes:
      - {name: path, type: string, required: true, constraint: "repo-root relative; convention `versioning.toml`"}
      - {name: baseline_commit, type: "sha40 | \"pending\"", required: true, constraint: "literal `pending` at merge (a commit cannot contain its own sha); a follow-up `chore(release):` commit on main records the merge sha; never rewritten thereafter"}
      - {name: allowed_types, type: "string[]", required: true, values: [feat, fix, chore, docs, style, refactor, perf, test, build, ci, revert]}
      - {name: reserved_scopes, type: "string[]", required: true, constraint: "contains `release`; entries never map to components"}
      - {name: entries, type: "RegistryEntry[]", required: true, constraint: "exactly 10 — validated against workspace enumeration"}
    constraints:
      - "unknown keys at any level are validation errors (strict schema, Q1)"
    relationships:
      - {to: RegistryEntry, kind: "1..n", direction: "contains"}
  - name: RegistryEntry
    description: One versioned component row — a Cargo crate, Bun package, or the manifest-less wire contract.
    attributes:
      - {name: id, type: string, required: true, unique: true, constraint: "lowercase path segment"}
      - {name: kind, type: enum, required: true, values: [crate, bun, contract]}
      - {name: manifest_path, type: path, required: "iff kind in (crate,bun)", constraint: "exists on disk; carries literal semver"}
      - {name: constant_path, type: "file#NAME", required: "iff kind=contract", constraint: "resolves to a literal semver constant"}
      - {name: tag_prefix, type: string, required: true, unique: true, constraint: "ends in `-v`; tags = tag_prefix + semver"}
      - {name: changelog_path, type: path, required: true}
      - {name: globs, type: "pathglob[]", required: true, constraint: "unique across entries"}
      - {name: scopes, type: "string[]", required: true, constraint: "unique across entries; never overlap reserved_scopes"}
      - {name: depends_on, type: "id[]", required: true, constraint: "ids resolve to entries; DAG acyclic; no self-edge"}
    relationships:
      - {to: RegistryEntry, kind: "n..m", direction: "depends_on"}
```

## Entity summary

Two entities: `RegistryConfig` (the document — baseline sha, grammar vocabulary, entry list) and `RegistryEntry` (one row per component — identity, manifest/constant pointer, tag prefix, changelog path, attribution globs, scope aliases, internal edges). The `manifest_path`-xor-`constant_path` correspondence is keyed on `kind`. Everything is metadata; no version literals live here.
