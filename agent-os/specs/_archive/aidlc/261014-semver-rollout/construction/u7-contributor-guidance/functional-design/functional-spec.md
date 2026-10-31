# U7 Contributor Guidance — Functional Spec

## Purpose

Teaches the convention to both audiences: contributors/maintainers (docs) and agents (AGENTS.md + a skill + rules). Depends on U2 (grammar it documents), U5 (hook mechanics), U4 (release flow).

## Workflows

### WF-1 `docs/VERSIONING.md` — canonical reference (BR7.1)

Sections: components+versions model (independent, 0.1.0 baseline); commit grammar + type table; scope list with `versioning scopes` as authority (BR7.3); breaking markers; exemptions; the local hook (`make setup`, `--no-verify`); where changelogs live (per-component + root + WIRE-CHANGELOG); the release flow (BR7.5 mechanics); prerelease lanes (alpha/beta/nightly + dispatch).

### WF-2 Contributor entry points

- `CONTRIBUTING.md`: commit-convention section → links VERSIONING.md.
- `docs/DEVELOPMENT.md`, `docs/TESTING.md`: cross-link the convention + `make version-check`/`make commit-check` targets.

### WF-3 Agent surfaces (BR7.2)

- `AGENTS.md`: commit grammar + semver rules under contribution standards; name the hook + CI gate so agents know their commits are checked.
- New skill `.agents/skills/semver-conventions/SKILL.md` (or equivalent): the grammar, how to choose scopes, what triggers a bump, how to run `commit-check` locally before committing.
- `.cursor/rules/` and `AGENTS.md` hard-boundary notes updated where they describe commits.

## States

Docs are either in sync with the registry/workflows or not — drift is guarded by BR7.3 (reference-don't-duplicate) plus the existing doc-check gates.

## Interfaces

- Documents: U2 grammar, U5 hook+gate, U4 release flow, U1 registry.
- No code surface; produces Markdown + rules only.
