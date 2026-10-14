# Scope Definition — Questions

## Q1. Which deliverables should carry their own version number?

Context: the accepted intent assumption leaves the component inventory open — this pins the boundary for the versioned set and for how finely bumps propagate.

- A. Every workspace member — each Cargo crate and each Bun app/package versions independently
- B. Only consumer-facing deliverables — components that ship to users (ox CLI, web app, api-client, any crate consumers depend on); internal-only crates still propagate bumps to dependents
- C. Coarse units — a small set of named components versions; everything else rolls up under them
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — every workspace member versions independently. Chat: "all our crates/packages should have their own versioning be it just for internal changelog purposes. it also helps with debugging since we can identify easier what change broke things." (2026-10-14T01:33:11Z, **Mode:** chat)

## Q2. Besides shipped components, should the API/RPC wire contract carry its own version?

Context: self-update and MCP clients may want to check protocol compatibility — that is a different axis from component versions and would add a second versioned surface.

- A. Yes — define a protocol/contract version alongside component versions
- B. No — component versions are enough for now
- C. Not yet defined
- X. Other (please specify)

[Answer]: A — define a protocol/contract version alongside component versions. Chat: "lets give api/rpc wire contract also its versioning." (2026-10-14T01:33:11Z, **Mode:** chat)

## Q3. Which of these is explicitly OUT of scope for this initiative? (select all that apply)

Context: draws the exclusion boundary so the scope document can say what this work deliberately does not cover.

- A. Publishing versioned artifacts to public registries (crates.io, npm)
- B. Backfilling changelogs for history before 0.1.0
- C. Versioned container images or distro packages beyond the existing release packaging
- D. All of the above are out
- E. None — everything listed is in scope
- X. Other (please specify)

[Answer]: D — all out. Explicit: registry publishing "will be handled later outside of this change". B and C read as out by implication — changelog history starts at 0.1.0 going forward, and no new versioned artifact types beyond the existing packaging were requested. Flagged for confirmation. (2026-10-14T01:33:11Z, **Mode:** chat)

## Q4. Of the confirmed capabilities, what is Must-have vs deferrable?

Context: the confirmed boundary covers commit-prefix convention, git hook, CI-side enforcement, bump automation, per-component + global changelogs, release wiring, agent skill/doc updates, and consistent version surfaces. This ranks them for the backlog.

- A. All confirmed capabilities are Must — nothing in the boundary is deferrable
- B. Core automation is Must; agent skill/doc updates can land as a fast follow
- C. Something else is deferrable (describe under Other)
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — all Must. Chat: "everything in the boundary is a must-have." (2026-10-14T01:33:11Z, **Mode:** chat)

## Q5. Are there hard deadlines tied to any capability?

Context: you mentioned versioning is a prerequisite for a future issue — this records whether that creates a date or just an ordering constraint.

- A. No deadline — land it correctly, not quickly
- B. Soft ordering — versioning must exist before the future issue that depends on it is picked up
- C. Hard date (describe under Other)
- D. Not applicable
- X. Other (please specify)

[Answer]: C — "as soon as possible", but quality-gated: solid quality and tests required; CI/Docker-related changes are tested locally with docker + act. Chat: "deadline is as soon as possible but with solid quality, tests and so on (use docker + act locally to tests CI/docker related changess)." (2026-10-14T01:33:11Z, **Mode:** chat)

## Q6. What sequencing preference should drive the work order?

Context: decides whether the backlog sorts by dependency chain, visible value, or risk retirement.

- A. Dependency-first — convention/detection, then bump logic, then changelogs, then release wiring, then docs
- B. Value-first — visible wins (versioned surfaces, changelogs) before enforcement plumbing
- C. Risk-first — hardest unknowns (dependency-aware change detection across two toolchains) first
- D. Not yet defined
- X. Other (please specify)

[Answer]: A — dependency-first, inferred from the capability chain (the convention must exist before detection, detection before bump logic, bump logic before changelogs and release wiring). Not explicitly stated by the user — flagged for confirmation. (2026-10-14T01:33:11Z, **Mode:** chat)

## Q7. Any nice-to-haves that belong in the backlog now rather than deferred? (select all that apply)

Context: captures stretch items while the boundary is being drawn; anything not selected stays out of this initiative.

- A. PR title/conversation linting for the same prefix convention
- B. Automated release PRs or bot-opened bump branches instead of manual version bumps
- C. Prerelease channels (alpha/beta/nightly version schemes)
- D. Signed or annotated release tags
- E. None — keep the backlog to the confirmed must-haves
- X. Other (please specify)

[Answer]: A, B, C, D — all included. Chat: "include the nice to haves you mentioned." (2026-10-14T01:33:11Z, **Mode:** chat)

## Consolidated Summary Confirmation

Summary of confirmed answers:

- Component inventory: every workspace member versions independently — each Cargo crate and each Bun app/package carries its own semver, starting at 0.1.0.
- Protocol version: the API/RPC wire contract gets its own version alongside component versions.
- Out of scope: registry publishing (explicitly deferred to a later change); changelog backfill before 0.1.0 and new versioned container/distro artifacts — read as out by implication, please confirm.
- Priority: everything in the confirmed boundary is Must-have.
- Deadline: as soon as possible, quality-gated — solid tests required; CI/Docker changes verified locally with docker + act.
- Sequencing: dependency-first (convention → detection → bump → changelog → release wiring → docs) — inferred, please confirm.
- Nice-to-haves in scope: PR-title linting, automated release PRs/bump branches, prerelease channels, signed/annotated tags.

Does this all look correct before I generate the artifact?

- Looks correct
- Request changes

[Answer]: Looks correct
