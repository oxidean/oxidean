# Oxpilot Agent — Shaping Notes (imported)

## Scope

An agentic chat/builder/debugger inside the forge: `/oxpilot` page plus
computerUse, letting end-users connect the agent to their local or remote
systems under their own configuration, on the TanStack `ai-octane`
framework.

## Decisions

- Server surface in `crates/oxidean-api-oxpilot` (domain-crate convention;
  RPCs under `/api/`)
- computerUse runs through a user-configured connector (gateway ↔ client) —
  no instance-held credentials for user machines
- Full AI-DLC `feature` scope: all 33 stages including market research,
  mockups, and operation stages
- Local unit verification deferred to CI per user directive (recorded in
  the intent state)

## Context

- **Origin framework:** AI-DLC intent — the last intent run before the
  Agent OS migration; its audit trail is the largest in the archive
- **Product alignment:** extends the agent-facing AGT-*/CLI-* surfaces on
  `agent-os/product/roadmap.md`

## Outcome

Shipped with green CI after CodeRabbit review-fix loops. Iterations on
/oxpilot (new tools, connector polish, UX) shape new specs here.
