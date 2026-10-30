# Intent Statement — `/oxpilot` Agentic Chat / Builder / Debugger

## Problem Statement

Oxidean users who want AI help with repository work — discussing code, making
changes, investigating failing builds — must leave the forge for external
agentic tools, losing the repository, issue, and CI context the forge already
holds. [desc] [Q1] [Q2]

The product lacks an in-place agentic surface: there is no page where a user
can hold a multi-turn agentic chat that can propose and apply code changes or
debug failures against their repositories, and no opt-in mechanism for a user
to point that agent at a local or remote system they control (computer use).
[desc] [Q1]

## Target Customer

Oxidean end users on both the cloud product and self-host deployments:
developers and maintainers who already use the forge for repositories, issues,
pull requests, and actions, and who feel the pain of fragmented tooling and
lost repository/CI context when switching to external AI tools. [Q2]

Secondary stakeholders: self-host operators who need the feature to be
optional/configurable and free of forced external service dependencies. [Q5]

## Success Metrics

Delivery success for this initiative, per the request: a new branch, a pull
request against `main`, all CI gates green, and a CodeRabbit review loop
driven to zero outstanding findings. [desc] [Q3]

Product success signals (measurable): [Q3]

- `/oxpilot` session completion rate — chat → proposed change → applied
  change or PR. [Q3]
- Computer-use connection success rate for user-configured local/remote
  targets. [Q3]
- Time-to-first-agent-reply on the `/oxpilot` page. [Q3]
- Zero credential or secret leaks through agent sessions (hard security gate,
  not a metric to optimize). [desc] [Q3]

## Initiative Trigger

Opportunity-driven: the TanStack `ai` monorepo's `ai-octane` package provides
an agent/streaming framework aligned with Oxidean's existing Octane (`.tsrx`)
UI stack, making a first-class in-product agent feasible without inventing a
custom framework — while competing forges and devtools already ship in-product
AI assistants. [desc] [Q4]

## Initial Scope Signal

Workflow-selected scope: `feature` — full lifecycle (ideation → inception →
per-unit construction → operation). [scope]

Confirmed product boundary [Q8] (conductor-confirmed under standing
instruction; ratifiable at PR review): confirmed `feature`. The initiative
covers a new product surface — a `/oxpilot` page hosting an agentic
chat/builder/debugger, backend agent/streaming work built on `ai-octane`,
user-configured computerUse connectivity to local or remote systems, and the
security/privacy design those connections require — delivered through the
existing branch → PR → green-CI pipeline. [desc] [Q8] [memory:M3]

Non-goal: `/oxpilot` does not replace the `ox` CLI or external agentic
tooling — it complements them. [Q1]

## Assumptions & Open Questions

- [assumption] The agent's backing LLM provider is operator-configurable
  (cloud and self-host may plug different providers); no provider was named in
  the request, so model selection is deferred to requirements/design.
- [assumption] Agent sessions and their transcripts are persisted per-user
  server-side so a chat survives reloads; not stated in the request.
- [assumption] Computer-use targets are connected and configured explicitly by
  the end user (never auto-discovered or enabled by an admin on their behalf),
  matching the request's "to their own liking and configuration". [desc]
- [assumption] "Builder/debugger" implies the agent can propose file changes
  and inspect CI/build output within the forge context; the exact action
  surface (patch proposal vs. direct commit vs. PR creation) is deferred to
  requirements-analysis.
