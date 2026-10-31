# Contract Design — Questions

Boundaries from `unit-of-work-dependency.md` plus the externally consumed surfaces (CLI dist manifest, `system.manifest` wire fields). Sources: unit-of-work.md, unit-of-work-dependency.md, domain-design/components.md, requirements.md.

## Q1. Which boundaries get formal contracts?

Every DAG edge is a candidate; the ones that carry data need real specs:

- `versioning.toml` schema — U1 produces, U2/U3/U4/U5/U6 consume (the foundational contract).
- Commit parser module + `commit-check` CLI surface — U2 produces, U3 + U5 consume.
- Release-plan shape — U3 produces, U4 consumes (also the release-PR workflow's payload).
- Tag grammar + workflow trigger contract — U4 produces, CI/GitHub consumes (external boundary).
- `latest.json` dist-manifest shape — U4 writes, U6's `/cli/latest` reads (external-ish: installers consume it too).
- `system.manifest` version fields + generated-client wire constant — U6 produces, External consumers (CLI/MCP clients, admins).

Options:
- A) All six, with fenced shared-schema/OpenAPI blocks per mechanism.
- B) Subset.

[Answer]: A — all six. The DAG is small and every edge crosses a real data boundary; `latest.json` and `system.manifest` are also external-facing contracts. Conductor-decided under standing continuation instruction; ratify at PR review.

## Q2. Mechanism per boundary?

- registry: shared-schema (TOML file contract, validated by `check`).
- parser/commit-check: shared-schema for the parsed-commit shape + a CLI contract (argv/stdout/exit codes) for the subcommand.
- release plan: shared-schema (JSON on disk/stdout — `plan` emits, `changelog`/`tag`/release-PR consume).
- tags/workflows: string-format contract (`<component>-v<semver>`) + workflow trigger matrix.
- `latest.json`: shared-schema (JSON file shape).
- `system.manifest`: OpenAPI-ish field contract (typed JSON RPC response fields).

Options:
- A) As listed.
- B) Different.

[Answer]: A — each mechanism matches how the repo already integrates (files, CLI, git tags, JSON RPC).

## Q3. Contract ownership?

Provider unit owns each spec: U1 owns `versioning.toml`; U2 owns parser+`commit-check`; U3 owns release-plan; U4 owns tag grammar + `latest.json` (it's the writer); U6 owns the manifest-field contract (it's the producer of wire_version et al.). Breaking-change rule: additive fields are safe (consumers ignore unknown); breaking changes bump the owning unit's contract section and must update all consumers in the same PR (units land together).

[Answer]: Provider-owns-spec; additive-safe consumers; breaking changes ship atomically in the single PR since all units land together — there is no cross-repo contract-drift window to legislate.

## Q4. Error/timeout/retry behaviour?

All boundaries are build-time/dev-time (files, CLI, git) — no network timeouts apply. Failure contract per surface: `check`/`commit-check` exit non-zero with a parseable error list (one per line `path:line: message`); `plan` fails on dirty semantics (cycle in DAG, unregistered workspace member, malformed manifest); writer subcommands are atomic per file (no partial manifest writes); changelog regeneration is idempotent (NFR2).

[Answer]: Fail-closed with typed error lists on stdout/stderr; atomic file writes; idempotent regeneration. No retries — surfaces are deterministic local operations.

---

## Consolidated Summary Confirmation

- Six contracts: registry TOML schema; parser+`commit-check`; release-plan JSON; tag grammar + workflow triggers; `latest.json`; `system.manifest`/generated-client wire fields.
- Mechanisms: shared-schema blocks, CLI contract, string-format tag grammar, field contract.
- Ownership: provider unit owns each spec; additive changes safe, breaking changes atomic in the shared PR.
- Failure model: fail-closed typed errors; atomic writes; idempotent regeneration; no network boundaries.

Does this all look correct before I generate `contract-summary.md`?

- Looks correct
- Request changes

[Answer]: Looks correct
