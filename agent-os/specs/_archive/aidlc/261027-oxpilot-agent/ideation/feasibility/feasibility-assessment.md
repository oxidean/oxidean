# Feasibility Assessment — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`,
`market-research/competitive-analysis.md`, `market-research/market-trends.md`,
`market-research/build-vs-buy.md`; answers Q1–Q7 in
`feasibility-questions.md` (conductor-decided under standing instruction
2026-10-27; ratify at PR review). Support perspectives — platform engineering
and compliance — are integrated inline per the stage's `inline` topology.

## Verdict

**Feasible.** Every building block lands on an existing seam, and the one
genuinely new mechanism (the user-side computer-use connector) has a
well-understood transport shape. The feature's risk concentrates in its
security/consent model and in the youth of `ai-octane`; both have explicit
retirement paths (see `raid-log.md`).

## Technical viability

### Agent framework — verified

`@tanstack/ai-octane@1.0.0` is published and maintained (npm registry
modified 2026-10-22); peer `octane >=0.12.0` is satisfied by
`apps/web`'s `octane ^0.15.0`. Provider adapters `@tanstack/ai-openai`,
`-anthropic`, `-ollama`, `-openrouter`, `-gemini` all sit at 1.0.0 — the
provider-configurable-LLM requirement (intent-capture assumption) is covered
including local models. The package ships uncompiled `.tsrx` that our Octane
plugin compiles — consistent with the mandated UI stack, no parallel
framework. Residual risk: first published 2026-08-21 — young; pinned versions
+ isolation behind a forge-tool layer + the recorded OpenHands fallback carry
it.

### Integration seams — all existing

- Chat surface: Octane route `/oxpilot` under the existing route-coverage
  manifest; `useChat`/`createChatHook` for UI state.
- Model traffic: SSE via `fetchServerSentEvents` through the existing `/api/`
  proxy path — per the learned rule, new API surface stays under `/api/` so
  Traefik/Caddy/web proxy tables need no edge-parity work.
- Forge actions: new `oxpilot.*` RPC procedures on the existing dispatch
  (`oxidean-api`), session-authenticated; builder/debugger tools call the
  same domain modules the REST façade uses.
- Session persistence: `oxidean-db` (dialect rules apply — SQL lives only in
  the db crate, migrations ×3).
- Deploy: same images/pipeline (`images.yml`, Railway IaC) — the feature is a
  flag-gated addition to `api` + `web`, not a new service.

### Computer-use transport — feasible, new

The user-side connector **dials out** to the forge: outbound-only from the
user's system, so no inbound listener/NAT traversal is needed on local or
remote targets. The forge side terminates an authenticated, consented
channel; credential custody stays user-side (PAT-class per the project
mandate — machine transports never take session cookies). Transport detail
(WebSocket vs polled SSE) is a domain-design decision; both fit the existing
edge. This is the feature's one novel mechanism — no vendor product offers
user-hosted targets (`build-vs-buy.md` Decision B).

## Platform-engineering lens

- No new deployable required: `/oxpilot` rides `apps/web`; agent dispatch
  rides `oxidean-api`. A separate agent service was explicitly rejected (Q1-B)
  — it would double auth, edge, and deploy surface for no benefit.
- Long-lived channels: SSE/WS connections through `oxidean-web` need
  proxy-read-timeout and connection-drain review at infrastructure-design;
  Railway and self-host Compose both support them but timeouts differ.
- Provider egress: cloud deployment needs outbound HTTPS to the configured
  provider; self-host may run fully offline with Ollama — egress must be
  documented per provider in `docs/CONFIGURATION.md`.
- Cost shape: token spend flows through operator-configured provider keys —
  no platform billing component in v1.

## Compliance lens

- No PCI/HIPAA/GDPR-specific obligation identified for this feature at
  ideation depth; a formal scan is queued for nfr-requirements (carried
  assumption in market-trends).
- Standing obligations: explicit user consent + authorization for computer
  use (never enabled on the user's behalf — accepted assumption); transcript
  redaction/retention policy; provider egress disclosure; PAT/pubkey-class
  credentials on the connector; audit logging of agent actions.
- Secrets posture: transcripts and provider calls must not leak credentials —
  redaction design lands in nfr-requirements; the repo rule against secrets
  in commits/examples covers our own artifacts.

## What would change the verdict

- `ai-octane` breaking API changes or abandonment → OpenHands Agent Server
  fallback (recorded in `build-vs-buy.md`); changes the build, not the
  product decision.
- A vendor shipping user-hosted computer-use targets → revisit Decision B in
  `build-vs-buy.md`.
- Discovery that the forge's edge cannot hold long-lived channels under the
  existing proxy config → escalate to infrastructure-design before the
  computer-use unit.

## Assumptions & Open Questions

- [assumption] `ai-octane` at 1.0.0 is treated as feasible for the full
  delivery window; its documented surface (useChat, createChatHook,
  toolDefinition .server()/.client(), approvals, fetchServerSentEvents) was
  verified against the registry but not yet exercised in-repo — an early
  construction-unit spike is the retirement action.
- [assumption] The existing `/api/` proxy path is assumed to support SSE
  streaming without edge changes; if it does not, an edge-config change
  becomes a new unit — spike validates this first.
- [assumption] No dedicated team, budget, or deadline constraint applies;
  delivery proceeds under the standing conductor delegation through the
  existing PR/CI pipeline.
