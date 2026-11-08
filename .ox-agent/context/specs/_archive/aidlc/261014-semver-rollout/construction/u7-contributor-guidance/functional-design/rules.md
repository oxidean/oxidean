# U7 Contributor Guidance — Business Rules

Source of truth: the fenced YAML block.

```yaml
rules:
  - id: BR7.1
    statement: "The full contribution loop is documented: grammar + allowed types, scope list + how to pick one, breaking markers, exempt patterns, hook install via `make setup`, `--no-verify` bypass, where changelogs land, and the release/tag/PR flow including prerelease lanes."
    category: policy
    applies_to: DocSurface.docs_versioning
    trigger: "doc authoring"
    logic: "docs/VERSIONING.md carries every topic in the enum; CONTRIBUTING.md links to it rather than duplicating it."
    violation: n/a
    source: FR7, FR7.2
  - id: BR7.2
    statement: "Agent-facing surfaces carry the same convention: AGENTS.md documents the commit grammar under contribution standards; a dedicated in-repo skill teaches the grammar + workflow; cursor/agent rules that describe commits are updated to name the convention."
    category: policy
    applies_to: DocSurface.agent_skill, DocSurface.agents_md
    trigger: "doc authoring"
    logic: "agents writing commits get identical guidance to humans — the hook/CI checks are named so agents understand what 'invalid commit' means."
    violation: n/a
    source: FR7.1, FR7.3
  - id: BR7.3
    statement: "Docs never duplicate mechanically-maintained truth: the scope-alias table in VERSIONING.md is illustrative and names `versioning.toml`/`versioning scopes` as authoritative — no hand-maintained copy that can drift."
    category: constraint
    applies_to: GuidanceDoc
    trigger: "doc authoring"
    logic: "IF a doc table can be generated from a machine source THEN the doc references the command/file instead of hard-coding the full table."
    violation: n/a
    source: FR7.3, NFR (single source of truth)
  - id: BR7.4
    statement: "Every doc touch is a `docs`/`chore`-typed commit — guidance changes themselves obey the grammar they describe and never induce a component bump on their own."
    category: policy
    applies_to: the unit's own commits
    trigger: "committing this unit's work"
    logic: "doc files land outside component globs (or under `docs:`-typed commits) — the versioning system must not dogfood itself into a spurious bump from this unit."
    violation: n/a
    source: FR7, BR3.4 housekeeping rule
  - id: BR7.5
    statement: "The release-flow doc names the real mechanics: `make release-plan`/`make release`/`make release-tags`, the `RELEASE_TOKEN` secret for tag cascades (GITHUB_TOKEN prohibited), prerelease channel triggers via workflow_dispatch, and the reserved `chore(release):` convention."
    category: policy
    applies_to: DocSurface.docs_versioning, maintainer audience
    trigger: "doc authoring"
    logic: "maintainer-facing docs name the commands, the secret name, and the exemption grammar — enough to operate a release without reading source."
    violation: n/a
    source: FR7.2, FR8.2
```

## Rules summary

| Rule | Category | In one line |
|---|---|---|
| BR7.1 | policy | VERSIONING.md covers the full loop |
| BR7.2 | policy | agents get the same convention as humans |
| BR7.3 | constraint | docs reference, never duplicate, machine truth |
| BR7.4 | policy | this unit's own commits obey the grammar |
| BR7.5 | policy | release ops doc names real mechanics + RELEASE_TOKEN |
