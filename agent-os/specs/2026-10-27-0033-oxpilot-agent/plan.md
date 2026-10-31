# Plan — Oxpilot Agent (imported record)

> Imported 2026-10-31. Source: AI-DLC intent `261027-oxpilot-agent`
> ([`../_archive/aidlc/261027-oxpilot-agent/`](../_archive/aidlc/261027-oxpilot-agent/)).
> All 33 stages completed.

## Status: shipped via #203 (merged 2026-10-30)

Self-driven build of an agentic chat/builder/debugger: a `/oxpilot` page
with computerUse — end-users connect the agent to their local or remote
systems under their own configuration — built on TanStack `ai-octane` with
attention to standards, UI/UX, security/privacy, and usability. Delivered
self-driven to a green-CI PR with CodeRabbit review-fix loops.

## Delivered units

- `session-core` — session model + persistence (migrations 0085–0093)
- `agent-runtime` + `providers` — provider abstraction and runtime
- `oxpilot-tools` — agent tool surface
- `connector-gateway` / `connector-client` — computerUse channel to
  user-managed systems
- `oxpilot-ui` — `/oxpilot` pages, chat components, admin surface
- `skeleton-slice` — first end-to-end slice run through all applicable
  stages (skeleton stance: on)
