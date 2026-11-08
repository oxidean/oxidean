# Requirements Analysis Questions — `/oxpilot` Agentic Chat / Builder / Debugger

Upstream inputs: `ideation/intent-capture/intent-statement.md`,
`ideation/scope-definition/scope-document.md`, `codekb/business-overview.md`,
`codekb/architecture.md`, `codekb/code-structure.md`,
`inception/practices-discovery/team-practices.md` (all six consumed).

## Sources

- [desc] Initial description: "Fully self-driven I want you to plan and build a full setup of a agentic chat/builder/debuger that has its own /oxpilot page, has computerUse (that allows the end-user to connect the agent to their local system or a remote system to their own liking and configuration). For the framework I want to use https://github.com/TanStack/ai/tree/main/packages/ai-octane and please go for best effort around standards, UI design/UX, security/privacy and usability. I want you to self-drive this until we have a new branch with a PR against main with green CI. Once CI is green I want you to trigger coderabbit AI for review, poll for its response, fix them and do this until coderabbit doesnt find anything anymore. To trigger a review, check the comment it posts or just comment `@coderabbitai review` yourself"
- [scope] Workflow-selected scope: `feature`.

## Q1. Which LLM provider adapters ship in v1?

`@tanstack/ai` provider adapters at 1.0.0 cover OpenAI, Anthropic, and
community Ollama (verified in feasibility). Operators configure provider +
key via `OXIDEAN_OXPILOT_*` env vars (cloud default + self-host override).

- A. OpenAI + Anthropic + Ollama — the full verified adapter trio covers cloud, enterprise, and self-host/local-model cases with no extra adapter work.
- B. OpenAI + Anthropic only — Ollama waits; self-host operators without a key can't run it.
- C. Ollama + one cloud provider — self-host-first, smallest surface.
- D. OpenAI only — simplest possible v1.
- X. Other (please specify)

[Answer]: A — OpenAI + Anthropic + Ollama: full verified adapter trio; covers cloud, enterprise, and local-model self-host with no extra adapter work. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. What is the builder's action surface in v1?

Rough mockups show both "Show patch" and "Create branch + PR". Repo write
capabilities exist (`oxidean-vcs` changesets, branch/PR RPC).

- A. Both, as decided in mockups: the agent proposes a reviewable diff in-session AND, on explicit user action, materializes it as a branch + PR against the repo; it never pushes or merges on its own.
- B. Patch/diff proposal only — branch+PR is a follow-up feature.
- C. Branch + PR only — no in-session diff preview.
- D. Not decided — leave to domain design.
- X. Other (please specify)

[Answer]: A — both: in-session reviewable diff AND explicit-user-action branch + PR; never autonomous push/merge. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. What is the computer-use connector's capability bound in v1?

The connector is a small binary/service the user runs on their own system
(local or remote), dialing out to the forge. Capabilities to bound: shell
command execution, filesystem read/write, process inspection, GUI/screenshot
control.

- A. Shell exec + filesystem read/write only — the pragmatic developer-agent surface; GUI/screenshot control is out of v1 (huge blast radius, niche value); per-action consent still applies.
- B. Shell exec + fs + process list/inspect — adds observability for debugging flows.
- C. Full computer-use including GUI/screenshot — maximal parity with Anthropic-style computer use.
- D. Not decided — leave to design.
- X. Other (please specify)

[Answer]: A — shell exec + filesystem read/write only; GUI/screenshot out of v1; per-action consent applies. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Session persistence and transcript retention?

Sessions/transcripts land in `oxidean-db` (per practice: dialect migrations
×3). What retention model ships in v1?

- A. Indefinite retention, user-deletable — sessions persist until the user deletes them; operator retention knobs are a later feature. Simplest honest model, matches how issues/PRs persist.
- B. Configurable retention via `OXIDEAN_OXPILOT_*` env (default e.g. 90 days) plus user deletion.
- C. Ephemeral sessions only — nothing survives the session.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — indefinite retention, user-deletable; operator retention knobs deferred. Matches issue/PR persistence model. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Who can use `/oxpilot`?

- A. Any authenticated user on an instance where the operator enabled the feature flag and configured a provider — per-user sessions, per-user rate limits, no additional role gate in v1.
- B. Admin/owner roles only in v1.
- C. Per-user opt-in on top of the feature flag.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — any authenticated user when flag+provider are set; per-user sessions and rate limits; no role gate v1. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. Debugger scope in v1?

Actions read APIs exist (job-log SSE verified). How deep does the debugger go?

- A. Inspect failed Actions runs — list runs/jobs, fetch logs, summarize failures, propose fixes through the same reviewable-proposal path as the builder. Read-only CI surface in v1.
- B. A plus re-run triggering (`repo.actions.rerun*` procs exist) behind consent.
- C. Full run lifecycle control (rerun, cancel, approve gates).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — read-only CI surface in v1: list runs/jobs, fetch logs, summarize failures, propose fixes via the same reviewable path; no rerun in v1. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q7. Transcript content protection?

Transcripts may contain secrets the user pastes or tool results pull in.

- A. Warn-and-redact posture: secrets-pattern redaction on write (log-style scrubber) + "transcripts are stored plaintext otherwise" disclosure; no encryption at rest beyond DB-level.
- B. Full field-level encryption for transcripts (AES-256-GCM env-key chain).
- C. Nothing special — standard row storage.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — warn-and-redact: secrets-pattern scrub on write + plaintext-storage disclosure; no field encryption in v1. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q8. Rate-limiting / quota shape?

Per-process sliding-window limiters exist to clone. Operators need cost
control on LLM spend.

- A. Per-user + per-instance configurable message/token budgets via `OXIDEAN_OXPILOT_*` env, defaults generous; single-replica caveat recorded (limiters are per-process).
- B. Per-user fixed defaults only, no operator knob.
- C. No rate limits in v1.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — per-user + per-instance budgets via OXIDEAN_OXPILOT_* env; per-process limiter caveat recorded. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Providers: OpenAI + Anthropic + Ollama adapters (operator-configured) (Q1).
- Builder: reviewable diff AND explicit-action branch+PR; never autonomous (Q2).
- Connector v1: shell exec + fs read/write; no GUI; per-action consent (Q3).
- Retention: indefinite, user-deletable; no operator knob v1 (Q4).
- Access: any authenticated user when flag+provider set; no role gate (Q5).
- Debugger: read-only CI inspect + fix proposals; no rerun (Q6).
- Transcripts: secrets-pattern redaction + plaintext disclosure (Q7).
- Limits: per-user + per-instance env-configured budgets (Q8).

Does this all look correct before I generate the requirements artifact?

- Looks correct
- Request changes

[Answer]: Looks correct
