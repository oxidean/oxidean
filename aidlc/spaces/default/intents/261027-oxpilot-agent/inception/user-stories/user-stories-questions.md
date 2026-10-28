# User Stories Questions — `/oxpilot`

Upstream inputs: `inception/requirements-analysis/requirements.md` (FR1–FR7,
NFR1–NFR9), `codekb/business-overview.md`, `codekb/component-inventory.md`,
`inception/practices-discovery/team-practices.md`.

## Sources

- [desc] Intent: `/oxpilot` agentic chat/builder/debugger with opt-in computer use.
- [scope] Workflow-selected scope: `feature`.

## Q1. Persona set for the stories?

Candidate personas from the intent + stakeholder map: the driving end user
(runs sessions, approves changes), the connecting user (registers/consents
computer-use targets — may be the same human but a distinct role hat), the
self-host operator (enables/configures the feature), the PR reviewer
(consumes agent-produced branches/PRs).

- A. Four personas: driving user, connecting user, operator, PR reviewer — covers every FR group distinctly.
- B. Three personas: merge driving+connecting into one "agent user" — the roles share a human but differ in consent context.
- C. Two personas: user + operator — leanest split.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — four personas: driving user, connecting user, operator, PR reviewer. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Story breakdown approach?

- A. By capability area (mirrors FR groups): sessions/chat, builder, debugger, computer-use, operator/enablement — clean FR traceability.
- B. By persona — stories grouped under each user type.
- C. By workflow (happy path first, edge/secondary later).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — by capability area, mirroring FR groups (sessions/chat, builder, debugger, computer-use, operator/enablement). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Story granularity?

- A. Feature-sized stories (~15–25 total) matching unit-planning granularity — each story ≈ a construction slice.
- B. Fine-grained (~40+) — one story per AC cluster.
- C. Epic-level (~8) — stories collapse into epics.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — feature-sized stories (~15-25), one per construction slice. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. MoSCoW priorities?

- A. Mirror the scope document's MoSCoW exactly (Must = named capabilities; Should = polish; Could = deferred nice-to-haves) — consistent single source.
- B. Re-prioritize per story value.
- C. Not decided.
- X. Other (please specify)

[Answer]: A — mirror the scope document's MoSCoW exactly. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Personas: 4 (driving user, connecting user, operator, PR reviewer) (Q1).
- Breakdown: by capability area mirroring FR groups (Q2).
- Granularity: feature-sized, ~15–25 stories (Q3).
- Priorities: mirror scope-document MoSCoW (Q4).

Does this all look correct before I generate personas and stories?

- Looks correct
- Request changes

[Answer]: Looks correct

## Q5 — Learnings ritual

Anything worth promoting to durable practice from this stage?

- A. The review-file format contract (exact Iteration line + findings table columns) — persist.
- B. Nothing beyond that.

[Answer]: A — Conductor-decided under standing instruction; ratify at PR review.
