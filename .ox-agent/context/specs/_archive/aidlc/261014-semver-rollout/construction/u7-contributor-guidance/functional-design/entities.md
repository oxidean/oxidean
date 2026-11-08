# U7 Contributor Guidance — Entities

Source of truth: the fenced YAML block.

```yaml
entities:
  - name: GuidanceDoc
    description: A documentation surface carrying the convention to its audience — human docs or agent rules.
    attributes:
      - {name: path, type: path, required: true}
      - {name: audience, type: enum, required: true, values: [contributor, maintainer, agent]}
      - {name: role, type: enum, required: true, values: [canonical_reference, entry_point, agent_rule, skill]}
      - {name: covers, type: "topic[]", required: true, values: [grammar, scopes, exemptions, hook_install, hook_bypass, changelogs, release_flow, prerelease_lanes, registry_map]}
    constraints:
      - "docs describe; versioning.toml prescribes — scope tables in docs are illustrative, the registry/`versioning scopes` output is authoritative"
    relationships: []
  - name: DocSurface
    description: The concrete file set this unit touches.
    attributes:
      - {name: docs_versioning, type: path, required: true, constraint: "docs/VERSIONING.md — canonical reference"}
      - {name: contributing, type: path, required: true, constraint: "CONTRIBUTING.md — links to VERSIONING.md"}
      - {name: agents_md, type: path, required: true, constraint: "AGENTS.md — commit convention added to hard-boundary/contribution sections"}
      - {name: dev_docs, type: path, required: true, constraint: "docs/DEVELOPMENT.md + docs/TESTING.md cross-links"}
      - {name: agent_skill, type: path, required: true, constraint: "in-repo skill file teaching agents the grammar (e.g. .agents/skills/semver-conventions/SKILL.md)"}
    relationships:
      - {to: GuidanceDoc, kind: "1..n", direction: "comprises"}
```

## Entity summary

`GuidanceDoc` (an authored doc/rule for an audience) and `DocSurface` (the concrete file set — VERSIONING.md canonical reference, CONTRIBUTING link, AGENTS.md agent rules, dev-docs cross-links, a dedicated agent skill).
