# U1 Version Registry — Business Rules

Source of truth: the fenced YAML block. Summary table follows.

```yaml
rules:
  - id: BR1.1
    statement: "Every registry entry's kind determines its version carrier: kind=crate|bun requires manifest_path with a literal semver field; kind=contract requires constant_path pointing at a literal semver constant in source."
    category: validation
    applies_to: RegistryEntry
    trigger: "`check` runs (locally, in CI gate, or before plan/bump)"
    logic: "IF kind is crate|bun THEN manifest_path must exist AND its version field is a literal semver (not `version.workspace = true`, not a variable) ELSE-FAIL; IF kind=contract THEN constant_path resolves to a literal semver ELSE-FAIL. manifest_path on a contract kind (or constant_path on crate/bun) is a schema violation."
    violation: "exit 1; `versioning.toml:<line>: <message>`"
    source: FR2.1, FR2.3
  - id: BR1.2
    statement: "Registry completeness is bidirectional: every Cargo workspace member (from `cargo metadata`) and every Bun workspace package (from root `package.json` workspaces globs) maps to exactly one entry; every entry maps to an existing member or the contract kind."
    category: validation
    applies_to: RegistryConfig
    trigger: "`check` runs"
    logic: "ENUMERATE workspace members and registry entries; IF a member lacks an entry → FAIL; IF an entry names a nonexistent manifest → FAIL; a private root package.json with no version and no publishable artifact is exempt by policy."
    violation: "exit 1; names the unregistered member / dangling entry"
    source: FR2.2
  - id: BR1.3
    statement: "Unknown keys anywhere in `versioning.toml` are validation errors — strict schema, no silent ignores."
    category: validation
    applies_to: RegistryConfig
    trigger: "`check` runs"
    logic: "IF a key is not in the schema THEN FAIL with the key path. Additive schema growth is deliberate (schema version), never accidental."
    violation: "exit 1"
    source: NFR3
  - id: BR1.4
    statement: "`depends_on` edges form a DAG: ids resolve to entries, no self-edges, no cycles."
    category: constraint
    applies_to: RegistryEntry.depends_on
    trigger: "`check` runs"
    logic: "IF an edge id is unknown, self-referential, or closes a cycle THEN FAIL naming the edge."
    violation: "exit 1"
    source: FR2.2
  - id: BR1.5
    statement: "Non-component version fields are never touched: lockfile schema markers (`skills-lock.json` `\"version\": 1`), scaffold/template manifests, test fixtures, and the versionless private root `package.json` are outside the registry and must not match any entry's manifest_path or glob."
    category: constraint
    applies_to: RegistryEntry.globs, RegistryEntry.manifest_path
    trigger: "`check` runs"
    logic: "IF a manifest_path targets a non-component file OR a glob would attribute files inside a fixture/template directory without an explicit exclusion THEN FAIL."
    violation: "exit 1"
    source: FR2.4
  - id: BR1.6
    statement: "The baseline commit is a static recorded sha — a commit cannot contain its own sha, so the merge ships `baseline_commit = \"pending\"` and a follow-up `chore(release):` commit on main records the merge sha; never computed, never rewritten by tooling after that."
    category: constraint
    applies_to: RegistryConfig.baseline_commit
    trigger: "registry load"
    logic: "IF baseline_commit is absent or is neither `pending` nor a 40-hex sha THEN FAIL; IF `pending` THEN check warns but exits 0 (main stays green in the recording gap) and the scan lower bound falls back to repo root (FR3.5 fallback, consumed by U3); IF a sha THEN it is the lower bound for all commit-range scans."
    violation: "exit 1 on absent/malformed; warning only on `pending`"
    source: FR2.5, FR3.5
  - id: BR1.7
    statement: "Scope aliases, tag prefixes, and attribution globs are globally unique: no two entries may claim the same scope alias, tag prefix, or an overlapping glob (a changed file must attribute to at most one entry via globs); reserved scopes may never appear as an entry alias."
    category: validation
    applies_to: RegistryEntry.scopes, RegistryEntry.tag_prefix, RegistryEntry.globs
    trigger: "`check` runs"
    logic: "IF two entries share a scope alias or tag prefix, or their glob sets overlap such that one changed path could match two entries, or an entry claims a reserved scope → FAIL naming the collision."
    violation: "exit 1"
    source: FR1, FR2.4, FR3
  - id: BR1.8
    statement: "`check` output is dual-format: human-readable `file:line: message` lines on stderr by default; `--json` emits the structured violation list for CI annotation. Exit codes: 0 clean, 1 violations, 2 usage/IO error."
    category: policy
    applies_to: check subcommand
    trigger: "every invocation"
    logic: "IF violations exist THEN print each as `versioning.toml:<line>: <msg>` and exit 1 ELSE print ok summary and exit 0."
    violation: n/a
    source: FR1.2, NFR4
```

## Rules summary

| Rule | Category | In one line |
|---|---|---|
| BR1.1 | validation | kind → version carrier (manifest literal vs constant) |
| BR1.2 | validation | workspace↔registry bidirectional completeness |
| BR1.3 | validation | strict schema — unknown keys fail |
| BR1.4 | constraint | dependency edges form an acyclic DAG |
| BR1.5 | constraint | fixtures/templates/schema-markers never versioned |
| BR1.6 | constraint | baseline sha recorded post-merge (`pending` sentinel until then) |
| BR1.7 | validation | scopes, tag prefixes, and globs globally unique |
| BR1.8 | policy | `check` output: text or `--json`; exit 0/1/2 |
