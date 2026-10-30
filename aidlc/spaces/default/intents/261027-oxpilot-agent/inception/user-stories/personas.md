# Personas — `/oxpilot`

Upstream: `inception/requirements-analysis/requirements.md`,
`codekb/business-overview.md`, `codekb/component-inventory.md`,
`inception/practices-discovery/team-practices.md`.

## P1 — Dana, the driving user

- Role: authenticated Oxidean user (developer/maintainer on a hosted repo).
- Goals: describe an outcome in chat, get repository-aware help, review a
  proposed diff, turn it into a branch + PR with a click.
- Pain points: external agents lack repo/CI context; copy-pasting code and
  CI logs into chat windows; agent tools that push or merge without review.
- Context: works in the web UI; expects streamed feedback and a session she
  can return to.

## P2 — Casey, the connecting user

- Role: the same class of user wearing a different hat — someone who wants
  the agent to act on their own machine (laptop, workstation, VPS, CI box).
- Goals: register a target, understand exactly what it grants, consent per
  action/session, revoke instantly; run the connector as a binary/service on
  their own system.
- Pain points: tools that demand always-on access, opaque permission
  scopes, credentials stored unclearly, agents that can't reach private
  machines without opening inbound ports.
- Context: security-sensitive; reads consent dialogs carefully.

## P3 — Quinn, the self-host operator

- Role: instance administrator enabling `/oxpilot` for their deployment.
- Goals: a feature flag + provider env vars (OpenAI, Anthropic, or local
  Ollama), cost-control budgets, documented env surface, audit visibility.
- Pain points: features that force a cloud dependency, undocumented env
  vars, per-user keys he can't audit.
- Context: reads `docs/CONFIGURATION.md`; may run fully air-gapped.

## P4 — Riley, the PR reviewer

- Role: maintainer reviewing branches/PRs the agent materialized.
- Goals: a normal PR — attributable author, clean diff, honest description
  of what the agent did; no special agent-specific review machinery.
- Pain points: unexplained bulk diffs, hidden tool provenance.
- Context: uses the standard PR review surface.

## Priority ranking

P1 > P2 > P3 > P4 — the driving user defines the core loop; the connecting
user defines the differentiating capability; the operator gates deployment;
the reviewer consumes output through existing surfaces and needs no new UI.
