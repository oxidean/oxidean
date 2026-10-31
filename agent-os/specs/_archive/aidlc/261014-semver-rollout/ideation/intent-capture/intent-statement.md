# Intent Statement — Per-Component Semver with Automated Changelog & Release Wiring

## Problem Statement

Oxidean ships as multiple components — Cargo crates, the web app, JS packages, and CLI/MCP surfaces — and its version numbers carry no reliable meaning today: consumers cannot tell what changed between releases, changelog and release production is manual, and the versioned surfaces (CLI self-update, MCP, API) have no trustworthy version to compare against. [desc] [Q1]

## Target Customer

Every consumer class is affected: self-hosted admins deciding whether to upgrade their deployment, contributors and maintainers coordinating changes in this repo, and downstream tooling consumers (ox CLI users, MCP clients, API consumers). [Q2]

## Success Metrics

- Pushing a version tag produces a release with a generated changelog and no manual edits. [Q4]
- Commit-prefix discipline lets tooling detect change types and drive version bumps automatically, enforced by a git hook. [desc] [Q4]
- Every consumer-facing surface reports its component's version consistently. [desc] [Q4]
- Each component carries its own semver — all starting at 0.1.0 — and bumps only when changes hit that component, where a change to a shared crate counts as a change to the components that depend on it. [Q3] [Q8]
- Changelogs exist per application/crate plus one global consolidated changelog that references the inner ones. [Q7]
- Agent skills and helpers follow the same commit-prefix practice. [desc]

## Initiative Trigger

Most primary functionality is now in place, which unlocks the possibility of people starting to use oxidean; versioning is also a prerequisite for a future issue. [Q5]

## Initial Scope Signal

- Workflow-selected scope (workflow-selected): `semver-release-automation` — a custom 15-stage plan covering semver conventions, automated changelog, release/CLI/MCP wiring, a commit-prefix git hook, and agent skill/helper updates. [scope]
- User-confirmed product boundary: the same coverage, plus change detection that treats a change to a shared crate as a change to the components that depend on it. [Q8]

## Assumptions & Open Questions

- Which concrete deliverables count as "components" for per-app versioning (each Cargo crate, apps/web, packages/api-client, the distributed ox CLI) is not yet enumerated; requirements-analysis and units-generation will pin it. [assumption]
- The "future issue" that needs versioning was mentioned but not named; it does not change the confirmed boundary. [assumption]
