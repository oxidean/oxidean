# Intent Capture & Framing — Questions

## Sources

- [desc] Initial description: "Fully self-driven I want you to plan and build a full setup of a agentic chat/builder/debuger that has its own /oxpilot page, has computerUse (that allows the end-user to connect the agent to their local system or a remote system to their own liking and configuration). For the framework I want to use https://github.com/TanStack/ai/tree/main/packages/ai-octane and please go for best effort around standards, UI design/UX, security/privacy and usability. I want you to self-drive this until we have a new branch with a PR against main with green CI. Once CI is green I want you to trigger coderabbit AI for review, poll for its response, fix them and do this until coderabbit doesnt find anything anymore. To trigger a review, check the comment it posts or just comment `@coderabbitai review` yourself"
- [scope] Workflow-selected scope: `feature`.
- [memory:M1] `aidlc/spaces/default/memory/project.md#Mandated`: "ALWAYS write every commit subject in the SemVer grammar `type(optional-scope)[!]: subject` (allowed types in `versioning.toml` `[grammar]`; `release` scope reserved) — enforced by `.githooks/commit-msg` (`make setup`) and CI `commit-lint` on the PR range and PR title; subjects drive per-component version bumps and changelogs. (affirmed 2026-10-18)"
- [memory:M3] `aidlc/spaces/default/memory/project.md#Mandated`: "ALWAYS reach `main` through a pull request — the `Main` ruleset admits only PR merges; the sole bypass actor is the release App for the stable-cut `chore(release):` commit. (affirmed 2026-10-18)"

## Q1. What business problem does the `/oxpilot` agentic chat/builder/debugger solve?

Context: Oxidean is a code forge (git hosting, issues, PRs, actions CI). The request adds an agent that can chat about, build, and debug code — with optional computer use against a user-connected system — inside that product.

- A. Give repository users an in-product AI assistant that can discuss their code, make changes (builder), and investigate failures (debugger), optionally acting on a user-connected local or remote machine via computerUse — so forge work that today needs an external AI tool (Cursor/Claude/Devin) can happen inside Oxidean itself.
- B. Replace the `ox` CLI and external AI tooling entirely — users would stop using local agents and do all agentic work through the hosted `/oxpilot` page.
- C. A general-purpose chatbot page for arbitrary Q&A, not tied to forge repositories or build/debug tasks.
- D. Only a CI log viewer with an LLM summarizer — no code changes, no computer use.
- E. Not yet defined.
- X. Other (please specify)

[Answer]: A — in-product agentic chat/build/debug over forge repositories, with opt-in computerUse onto user-connected systems; complements rather than replaces local tooling. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Who is the customer, and what pain do they have today?

Context: the request names an "end-user" who connects their own local or remote system for computer use, so the persona spans both cloud-hosted forge users and self-host operators.

- A. Oxidean users (cloud and self-host) — developers/maintainers who already use the forge for repos/issues/PRs/actions and today context-switch to external AI tools or manual work to triage failing builds, write patches, and get code review; the pain is fragmented tooling and losing repository context (code, issues, CI logs) that an in-product agent already has.
- B. Only Oxidean's own maintainers — an internal debugging tool for the oxidean-app codebase itself.
- C. Non-technical end users who want a chat UI and do not work with repositories or CI.
- D. Not identified.
- X. Other (please specify)

[Answer]: A — Oxidean end users on cloud and self-host; pain is tool fragmentation and loss of repository/CI context when they leave the forge to use external agents. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. What does success look like, and which metrics matter?

Context: the request defines a concrete done-state: a new branch, a PR against main, green CI, then a clean CodeRabbit review — plus product-level qualities (standards, UI/UX, security/privacy, usability).

- A. Deliverable success = a merged-ready PR against `main` with all CI gates green and zero outstanding CodeRabbit findings. Product success = an authenticated user can open `/oxpilot`, hold a multi-turn agentic chat that can propose and apply code changes and investigate failures, and can connect a local or remote computer-use target under their own control; measurable via: `/oxpilot` session completion rate (chat → proposed change → applied/PR), computer-use connection success rate, time-to-first-agent-reply, and zero credential/secret leaks in agent sessions (security gate).
- B. Success is only that the `/oxpilot` page renders — functional agent behavior, computer use, and CI outcomes are follow-on work outside this intent.
- C. Success is an external adoption metric (e.g. percentage of repos enabling the agent) with no in-repo acceptance criteria.
- D. Not yet defined.
- X. Other (please specify)

[Answer]: A — PR-based delivery success (branch + green CI + clean CodeRabbit pass, per the request) plus product success on the three capabilities: agentic chat, builder/debugger actions, and user-configured computerUse connectivity with no credential leakage. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. What is the trigger for this initiative — why now?

Context: the request names a specific new framework (TanStack `ai-octane`) as the desired foundation, which is itself a timing signal.

- A. Opportunity-driven: the TanStack `ai` monorepo's `ai-octane` package now provides an agent/streaming framework that matches Oxidean's Octane (`.tsrx`) UI stack, making a first-class in-product agent feasible without inventing a custom framework; competitive pressure from forges/devtools shipping in-product AI assistants (GitHub Copilot, GitLab Duo, Forgejo/Gitea ecosystem interest) makes this a differentiating feature for a forge.
- B. Regulatory or compliance-driven requirement.
- C. Tech-debt remediation of an existing agent implementation that is failing.
- D. Not identified.
- X. Other (please specify)

[Answer]: A — opportunity: `ai-octane` aligns with the existing Octane stack, and in-product agents are table stakes across competing forges/devtools. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Who are the key stakeholders, and what does each care about?

Context: the request was filed by the project's maintainer under a standing self-drive instruction with PR ratification; no other named stakeholders.

- A. Maintainer (the requester) — cares about standards, UI/UX quality, security/privacy of computer-use connections, usability, and a reviewable PR outcome ratified via CodeRabbit + green CI; also the decision-maker on scope and priority. Oxidean end users — care that the agent is trustworthy, that computer use only runs on systems they explicitly connect and configure, and that their credentials/code stay private. Oxidean self-host operators — care that the feature is optional/configurable and doesn't force external service dependencies on private deployments.
- B. Maintainer only; end users and self-host operators are not stakeholders for this feature.
- C. A separate product/design team owns UX decisions, and compliance owns the security posture — neither is represented in the request.
- D. Not identified.
- X. Other (please specify)

[Answer]: A — maintainer (requester, decision-maker, ratifies via PR), end users (trust, explicit-connect computer use, privacy), self-host operators (optionality, no forced external deps). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. Who decides scope or priority, and how are decisions ratified?

Context: the request delegates day-to-day decisions ("fully self-driven … expect no input until we have a PR that is fully ready") while reserving final ratification to PR review.

- A. The maintainer decides scope and priority; during this run they delegate in-stage decisions to the conductor under the standing self-driven instruction, and every conductor-made decision is marked for ratification at PR review (CodeRabbit pass + maintainer review of the PR).
- B. A committee or external approver must sign off each stage before work continues.
- C. Scope decisions are frozen by the initial request and cannot change during the run.
- D. Not identified.
- X. Other (please specify)

[Answer]: A — maintainer owns scope/priority; conductor decides under the standing self-drive instruction; all decisions ratifiable at PR review. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q7. Are there communication or reporting requirements for this work?

Context: the request names the reporting surface explicitly — a PR against main, CI status, and CodeRabbit review comments.

- A. Report through the PR itself: conventional SemVer-prefixed commits on a `type/<slug>` branch, green CI on the PR, `@coderabbitai review` triggering and response-loop until clean; the workflow's `aidlc/` artifact tree is the decision record; no separate status reports or cadence.
- B. Written status reports at a fixed cadence (e.g. daily summaries to a stakeholder channel) in addition to the PR.
- C. Not applicable — no reporting requirements.
- X. Other (please specify)

[Answer]: A — the PR is the reporting surface (SemVer commit subjects, green CI, CodeRabbit loop per the request); the aidlc record carries decision provenance. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q8. The workflow was started with the `feature` scope; does that scope match the intended product boundary?

Context: `feature` runs the full lifecycle (ideation → inception → per-unit construction → operation). The request includes computerUse plumbing, security/privacy design, and UI work — a multi-unit feature.

- A. Confirm `feature` — a full lifecycle fits: this is a new product surface with backend protocol work, UI, security design, and multiple units of construction; operations stages apply because the feature ships through the existing Railway/images pipeline.
- B. The intent is narrower than `feature` — e.g. only a UI page stub behind a flag (would fit `mvp`/`poc` better).
- C. The intent is broader than `feature` — e.g. it includes regulated-compliance work or org rollout (would fit `enterprise`).
- D. Not applicable — no product-boundary decision needed.
- X. Other (please specify)

[Answer]: A — `feature` confirmed: full-lifecycle new product surface with design, security, and per-unit construction; it ships through the existing deploy pipeline. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Problem: in-product agentic chat/builder/debugger on `/oxpilot` over forge repositories, with opt-in computerUse against user-connected local or remote systems (Q1).
- Customer: Oxidean end users on cloud and self-host whose agentic work today fragments across external tools and loses repository/CI context (Q2).
- Success: a branch + PR against `main` with green CI and a clean CodeRabbit loop; product success on chat, builder/debugger actions, and user-configured computerUse with no credential leakage (Q3).
- Trigger: `ai-octane` now matches the Octane stack; in-product agents are table stakes for forges (Q4).
- Stakeholders: maintainer (decides, ratifies via PR), end users (trust, explicit connect, privacy), self-host operators (optionality) (Q5).
- Decisions: maintainer owns scope/priority; conductor decides under the standing self-driven instruction, ratifiable at PR review (Q6).
- Reporting: the PR itself (SemVer commits, green CI, `@coderabbitai review` loop); aidlc record carries provenance (Q7).
- Scope: `feature` confirmed — full lifecycle for a new product surface shipping through the existing deploy pipeline (Q8).

Does this all look correct before I generate the artifact?

- Looks correct
- Request changes

[Answer]: Looks correct

## Assumption Confirmation

The artifacts carry these assumptions:

- [assumption] The agent's backing LLM provider is operator-configurable (cloud and self-host may plug different providers); no provider was named in the request, so model selection is deferred to requirements/design.
- [assumption] Agent sessions and their transcripts are persisted per-user server-side so a chat survives reloads; not stated in the request.
- [assumption] Computer-use targets are connected and configured explicitly by the end user (never auto-discovered or enabled by an admin on their behalf), matching the request's "to their own liking and configuration". [desc]
- [assumption] "Builder/debugger" implies the agent can propose file changes and inspect CI/build output within the forge context; the exact action surface (patch proposal vs. direct commit vs. PR creation) is deferred to requirements-analysis.
- [assumption] No external reviewers, compliance officers, or design owners beyond the maintainer are involved in this initiative; if a dedicated design reviewer exists, UX sign-off authority should be confirmed.

- A. Accept assumptions
- B. Convert to follow-up questions

[Answer]: A. Accept assumptions
