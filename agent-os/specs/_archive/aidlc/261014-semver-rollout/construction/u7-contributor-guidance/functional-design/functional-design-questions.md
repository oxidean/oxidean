# Functional Design — U7 Contributor Guidance — Questions

Unit: `u7-contributor-guidance` (kind: unset → full matrix). Scope: CONTRIBUTING/docs for the commit grammar + release flow, agent-facing skill/rule updates, `make setup` hook documentation. FRs: FR7, FR7.1–FR7.3.

## Q1. Where does the convention doc live?

- A) `docs/VERSIONING.md` — canonical reference (grammar, scopes, bump rules, release flow, channel lanes); CONTRIBUTING.md links to it.
- B) Inline in CONTRIBUTING.md.

[Answer]: A — a dedicated `docs/VERSIONING.md` keeps CONTRIBUTING short; the doc is the single source humans and agents both link to.

## Q2. Agent guidance — which surfaces get updated?

- A) AGENTS.md "stack" + contribution sections, `.agents/skills/` (new `semver-conventions` skill or doc additions), and `.cursor/rules/` where they encode commit practices — agents get the same convention as humans.
- B) Docs only; agents infer.

[Answer]: A — the same convention surfaces agents already read (AGENTS.md hard boundaries + a dedicated skill entry). The commit-prefix rule belongs in agent guidance because agents write most commits.

## Q3. What must contributors know beyond the grammar?

- A) The type vocabulary + scope list, `!`/BREAKING for breaking, exempt patterns, the `--no-verify` bypass + `make setup` install, where changelogs live, and the release flow (who tags, how the PR flow works, prerelease channels).
- B) Grammar only.

[Answer]: A — the full loop: write the commit → hook validates → CI double-checks → merge → automation bumps + changelog + tags. Contributors shouldn't need tribal knowledge to predict what their commit will bump.

## Q4. Documentation of scope aliases — manual list or generated?

- A) `docs/VERSIONING.md` documents the alias table generated from `versioning.toml` (a `versioning scopes` listing command keeps docs honest); the table in docs is illustrative, registry is authoritative.
- B) Hand-maintained table only.

[Answer]: A — docs show the table + the canonical command; the registry stays the single source of truth, docs point at it rather than duplicating it mechanically.

---

## Consolidated Summary Confirmation

- Canonical reference at `docs/VERSIONING.md`; CONTRIBUTING links to it.
- Agent surfaces updated: AGENTS.md + in-repo skills + cursor rules for commit practice.
- Docs cover the whole loop (grammar, scopes, exemptions, hook install/bypass, changelogs, release flow, prerelease lanes).
- Scope-alias table in docs points at `versioning.toml`/`versioning scopes` as authoritative.

Does this all look correct before I generate the functional-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
