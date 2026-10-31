# Intent Capture — Questions

## Sources

- [desc] Initial description: "I want to move our app over to having proper semver (automated with changelog), have it hooked up to releases, cli/mcp updates and any other place where this would make sense. This should be uniform and I want our versioning to start with 0.1.0 (current), we should also have a git hook to enfore semver prefixed commits for detection and agents skills/helpers to follow this practice too."
- [scope] Workflow-selected scope: `semver-release-automation`.

## Q1. What problem does adopting uniform semver solve for this project?

Context: the description asks for proper semver with an automated changelog. This pins down which pain is driving the work so the intent statement names it accurately.

- A. Version numbers carry no meaning today — consumers cannot tell what changed between releases
- B. Releases and changelogs are manual and error-prone
- C. Versioned surfaces (CLI self-update, MCP, API) need a trustworthy version to compare against
- D. All of the above
- E. Not yet defined
- X. Other (please specify)

[Answer]: D — all of the above. Chat: "expect that all consumers will care about the versioning." (2026-10-14T01:09:25Z, **Mode:** chat)

## Q2. Who is the primary consumer of the version number and changelog?

Context: the intended audience shapes how the changelog is written and where the version surfaces.

- A. Self-hosted admins deciding whether to upgrade their deployment
- B. Contributors and maintainers coordinating changes in this repo
- C. Downstream tooling consumers (ox CLI users, MCP clients, API consumers)
- D. All of the above
- E. Not identified
- X. Other (please specify)

[Answer]: D — all of the above. Chat: "expect that all consumers will care about the versioning." (2026-10-14T01:09:25Z, **Mode:** chat)

## Q3. Does one version number cover the whole repo, or do parts version independently?

Context: the description says versioning should be "uniform" — that could mean one shared version across the Cargo workspace and Bun packages, or one consistent semver practice with per-component numbers. This decision shapes the whole design.

- A. One repo-wide version — a single source of truth every component shares
- B. One semver practice everywhere, but components may carry their own version numbers
- C. Not yet defined
- X. Other (please specify)

[Answer]: B — one semver practice everywhere, components carry their own version numbers. Chat: "each app should have its own versioning based on if there were changes that hit that app. they should all start from the same (current) version number though." (2026-10-14T01:09:25Z, **Mode:** chat)

## Q4. What does "done" look like for this change?

Context: defines the success criteria the intent statement will record.

- A. Pushing a version tag produces a release with a generated changelog and no manual edits
- B. Commit-prefix discipline lets tooling detect change types and drive version bumps automatically
- C. Every consumer-facing surface (releases, CLI, MCP) reports the same version
- D. All of the above
- E. Not yet defined
- X. Other (please specify)

[Answer]: D — all of the above, with the Q3 refinement: each surface reports its own component's version consistently (per-component numbers, not one shared number). (2026-10-14T01:09:25Z, **Mode:** chat)

## Q5. Why now — what makes 0.1.0 the right moment to set this up?

Context: identifies the trigger for the initiative record.

- A. Approaching the first release consumers will actually install — versioning must be right before it ships
- B. CLI distribution and self-update need trustworthy version comparison
- C. Establishing contributor conventions (commit prefixes, agent practices) while the project is young
- D. All of the above
- E. Not yet defined
- X. Other (please specify)

[Answer]: X — chat: "I want to have versioning now since we have most primary functionality present which unlocks the ability for possibly people starting to use oxidean. Also we kind of need the versioning to be present for a future issue to be picked up." (2026-10-14T01:09:25Z, **Mode:** chat)

## Q6. Who besides you has a stake in how versioning works, and who makes the final call on version policy?

Context: feeds the stakeholder map — decision-makers vs. people affected by the decision.

- A. Solo maintainer — I decide everything; no other stakeholders
- B. Small maintainer group — I decide; others are affected mainly as contributors
- C. A wider stakeholder group exists (describe under Other)
- D. Not identified
- X. Other (please specify)

[Answer]: B — I decide; others are affected mainly as contributors. Chat: "I have to primary call on version policy." (2026-10-14T01:09:25Z, **Mode:** chat)

## Q7. How should releases be communicated to consumers?

Context: determines whether the changelog is only a repo file or also feeds release notes elsewhere.

- A. A generated CHANGELOG.md in the repo is enough
- B. Release notes generated from the changelog on each release
- C. Both — repo changelog plus generated release notes
- D. Not applicable — no formal communication requirements
- X. Other (please specify)

[Answer]: X — chat: "release comms should have changelogs per application/crate and one global one that consolidated references to all the inner changelogs." (2026-10-14T01:09:25Z, **Mode:** chat)

## Q8. Does the `semver-release-automation` scope match your intended product boundary?

Context: this workflow runs a custom 15-stage plan covering semver conventions, automated changelog, release/CLI/MCP wiring, a commit-prefix git hook, and agent skill/helper updates. Confirm the boundary is right or describe what should differ.

- A. Confirm — the boundary is semver + changelog automation, release/CLI/MCP wiring, commit hook, and agent helpers
- B. Same workflow, but the product boundary differs (describe under Other)
- C. Not yet defined
- X. Other (please specify)

[Answer]: A — confirm, with an addition. Chat: "boundary checks should be smart as to also detect shared crates hits on their dependencies" — change detection must treat a change to a shared crate as a change to the components that depend on it. (2026-10-14T01:09:25Z, **Mode:** chat)

## Consolidated Summary Confirmation

Summary of confirmed answers:

- Problem: all three pains — meaningless version numbers, manual release/changelog process, versioned surfaces needing a trustworthy version; all consumers will care.
- Consumers: all of them — self-host admins, contributors/maintainers, downstream tooling users.
- Versioning model: one semver practice with per-component version numbers; every component starts at 0.1.0 and bumps only when changes hit it.
- Done: tag-driven releases with generated changelogs, commit-prefix detection driving bumps, every surface reporting its component's version consistently.
- Trigger: primary functionality is in place so people may start using oxidean; versioning is also a prerequisite for a future issue.
- Stakeholders: you hold the primary call on version policy; others are affected mainly as contributors.
- Release comms: per-application/crate changelogs plus one global consolidated changelog referencing the inner ones.
- Boundary: confirmed — plus change detection must count a shared-crate change as a change to the components that depend on it.

Does this all look correct before I generate the artifact?

- Looks correct
- Request changes

[Answer]: Looks correct

## Assumption Confirmation

The generated artifacts carry these open assumptions:

- Which concrete deliverables count as "components" for per-app versioning (each Cargo crate, apps/web, packages/api-client, the distributed ox CLI) is not yet enumerated; requirements-analysis and units-generation will pin it.
- The "future issue" that needs versioning was mentioned but not named; it does not change the confirmed boundary.
- No reporting cadence beyond the changelog artifacts was established.
- The stakeholder list beyond the primary maintainer and consumer classes is unconfirmed — a wider group was not identified.

- A. Accept assumptions
- B. Convert to follow-up questions

[Answer]: A. Accept assumptions
