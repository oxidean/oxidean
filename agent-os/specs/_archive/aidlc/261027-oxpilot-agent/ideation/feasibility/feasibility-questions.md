# Feasibility Questions — `/oxpilot` Agentic Chat / Builder / Debugger

Upstream inputs: `intent-capture/intent-statement.md`,
`market-research/competitive-analysis.md`, `market-research/market-trends.md`,
`market-research/build-vs-buy.md` (this stage consumes all four).

## Sources

- [desc] Initial description: "Fully self-driven I want you to plan and build a full setup of a agentic chat/builder/debuger that has its own /oxpilot page, has computerUse (that allows the end-user to connect the agent to their local system or a remote system to their own liking and configuration). For the framework I want to use https://github.com/TanStack/ai/tree/main/packages/ai-octane and please go for best effort around standards, UI design/UX, security/privacy and usability. I want you to self-drive this until we have a new branch with a PR against main with green CI. Once CI is green I want you to trigger coderabbit AI for review, poll for its response, fix them and do this until coderabbit doesnt find anything anymore. To trigger a review, check the comment it posts or just comment `@coderabbitai review` yourself"
- [scope] Workflow-selected scope: `feature`.

## Q1. What existing systems must `/oxpilot` integrate with?

Context: the forge already has an RPC dispatch layer (`domain.verb` procedures through `oxidean-api` → `oxidean-db`), session/PAT auth, an edge tier (`oxidean-web` static/meta/proxy), Actions CI, and the Octane `.tsrx` web app.

- A. Integrate through existing seams only: new `oxpilot.*` RPC procedures on the existing dispatch, session auth for the page, SSE streaming through the existing proxy path under `/api/`, TanStack Query session helpers for state, Octane route under the existing route-coverage manifest. No new top-level namespace (learned rule: prefer `/api/` paths so Traefik/Caddy/web proxy entries already cover it).
- B. Stand up a separate agent service with its own port, auth, and edge routes.
- C. Bypass RPC — direct DB access from a new service.
- D. Not yet assessed.
- X. Other (please specify)

[Answer]: A — integrate through existing seams only: new `oxpilot.*` RPC procedures on the existing dispatch, session auth, SSE under `/api/` (learned rule: prefer `/api/` so Traefik/Caddy/web proxy cover it), TanStack Query helpers, Octane route with route-coverage rows. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. What regulatory/compliance constraints apply?

Context: code + prompts flow to an external LLM provider; computer-use sessions could touch user credentials; transcripts may contain secrets or private code. No PCI/HIPAA scope, but SOC2-adjacent concerns and data-residency expectations exist for self-host adopters.

- A. No hard regulatory driver (no PCI/HIPAA/GDPR-specific obligation identified at ideation depth), but design obligations stand: consent + authorization model for computer use, redaction/retention policy for transcripts, provider egress made explicit to operators, PAT-class credential rules for connectors (project mandate: machine transports authenticate with PAT/pubkey-class credentials, never session cookies), audit logging of agent actions. A formal compliance scan lands in nfr-requirements.
- B. Full regulatory mapping (GDPR DPA, SOC2 controls, EU AI Act classification) required before any design.
- C. No constraints — the feature is exempt as an internal tool.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — no hard regulatory driver identified at ideation depth; design obligations stand: consent/authz model for computer use, transcript redaction/retention, explicit provider egress, PAT-class connector credentials (project mandate), agent-action audit log. Formal compliance scan in nfr-requirements. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Is the directed framework (`ai-octane`) feasible on the current stack?

Context: `apps/web` runs `octane ^0.15.0`; `@tanstack/ai-octane@1.0.0` declares peer `octane >=0.12.0` and depends on `@tanstack/ai-client ^1.0.0`; provider adapters (ai-openai/ai-anthropic/ai-ollama/ai-openrouter/ai-gemini) are all at 1.0.0. The package publishes uncompiled `.tsrx` compiled by the Octane plugin.

- A. Feasible — verified: package is real, at 1.0.0, actively maintained (registry modified 2026-10-22), peer range satisfied by `octane ^0.15.0`, and adapters cover the provider-configurability requirement including local models via Ollama/OpenRouter. Residual risk: young package (first published 2026-08-21) — pin exact versions, treat API churn as a maintenance line, keep the OpenHands fallback note from build-vs-buy.
- B. Not feasible — the package is too young; build bespoke on provider SDKs.
- C. Feasible only if we embed OpenHands Agent Server alongside.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — feasible and verified: `@tanstack/ai-octane@1.0.0` real and maintained (registry modified 2026-10-22), peer `octane >=0.12.0` satisfied by our `^0.15.0`, all provider adapters at 1.0.0 incl. Ollama/OpenRouter. Residual: young package — pin versions, isolate behind forge-tool layer, keep OpenHands fallback. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Is streaming + computer-use transport feasible through the existing edge?

Context: chat needs SSE/WebSocket-grade streaming through `oxidean-web` → `oxidean-api`; computerUse needs a persistent, authenticated channel between the forge and a user-side connector on the user's own machine or remote host.

- A. Feasible — SSE for model streaming already matches `fetchServerSentEvents` and the existing `/api/` proxy path; for computerUse, the user-side connector dials OUT to the forge (outbound-only from the user's system, no inbound listener required) over an authenticated channel — this sidesteps NAT/firewall and keeps credential custody user-side. Detailed transport (WS vs polled SSE) deferred to domain-design.
- B. Requires a new dedicated edge service and inbound listeners on user machines.
- C. Only feasible for local systems, not remote.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — feasible: SSE model streaming matches `fetchServerSentEvents` + existing `/api/` proxy; computer-use connector dials OUT to the forge (outbound-only, no inbound listener on user systems) over an authenticated channel. WS-vs-SSE detail deferred to domain-design. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. What are the LLM-provider and key-custody constraints?

Context: assumption carried from intent-capture — provider is operator-configurable. Cloud (Railway) and self-host (Compose) deployments differ in who holds provider keys and whether an LLM provider is even reachable.

- A. Operator-configured provider per deployment: cloud operator sets provider+keys via `OXIDEAN_*` env (documented per the env-var rule); self-host operator does the same, with Ollama-class local providers supported so the feature never forces an external dependency. Per-user provider override is out of scope for v1 — the deployment's agent capability is operator-governed.
- B. Every end user supplies their own provider keys stored server-side.
- C. Oxidean operates a shared LLM pool billing users per-token.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — operator-configured provider per deployment via documented `OXIDEAN_*` env vars; self-host may use Ollama-class local providers so no forced external dependency; per-user BYO-key out of scope for v1. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. What are the capacity/timeline/organizational constraints?

Context: delivery is a single conductor-driven workflow under standing delegation; there is no dedicated team allocation, and every stage must fit the existing CI/coverage/deploy pipeline.

- A. Constraints are process-shaped, not budget-shaped: changes must pass the existing gates (`make web-lint`, `make web-format-check`, route/browser coverage manifests, SemVer commits, green CI, CodeRabbit loop). No headcount, budget, or deadline constraint beyond that; sequencing should front-load the riskiest piece (computer-use connector + session streaming) after the page skeleton.
- B. A fixed deadline applies.
- C. Additional headcount is required.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — process-shaped constraints only: existing CI/coverage/format gates + CodeRabbit loop; no budget/headcount/deadline constraints; front-load computer-use connector + streaming after the page skeleton. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q7. What are the top technical risks and how are they retired?

Context: build-vs-buy flagged `ai-octane` maturity; intent-capture flagged the computer-use consent/security model; the stack adds a persistent user-connector channel the product has never had.

- A. Top risks — (1) computer-use authorization/consent/isolation model is novel attack surface (retire via dedicated security design in nfr-requirements/nfr-design + PAT-class connector credentials); (2) `ai-octane` API churn on a young 1.0 package (retire via exact version pinning + adapter isolation behind forge-tool layer + recorded OpenHands fallback); (3) long-lived streaming channels through edge/auth (retire via early spike: one real SSE round-trip through the `/api/` proxy in the first construction unit); (4) secret leakage into transcripts/provider calls (retire via transcript redaction policy + egress disclosure in nfr-requirements). All four land as RAID entries.
- B. Risks are minor; proceed without specific retirement actions.
- C. The risks are disqualifying — descope computerUse.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — four top risks each with retirement: (1) computer-use authz/consent/isolation → dedicated security design + PAT-class connector creds; (2) ai-octane churn → version pinning + forge-tool isolation + OpenHands fallback; (3) long-lived streaming through edge → early SSE spike in first construction unit; (4) secret leakage into transcripts/provider → redaction + egress disclosure in nfr-requirements. All land in the RAID log. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Integration: existing seams only — `oxpilot.*` RPC procedures, session auth, SSE under `/api/`, Octane route with coverage rows (Q1).
- Compliance: no hard regulatory driver; consent/authz, redaction/retention, PAT-class connector creds, audit log, explicit egress are design obligations; formal scan in nfr-requirements (Q2).
- Framework: `@tanstack/ai-octane@1.0.0` verified feasible — peer range satisfied, adapters at 1.0.0, Ollama/OpenRouter cover local+multi-provider; pin versions, OpenHands fallback recorded (Q3).
- Transport: SSE for model streaming; computer-use connector dials out to the forge — no inbound listener on user systems (Q4).
- Provider custody: operator-configured per deployment via `OXIDEAN_*` env; per-user BYO-key out of scope v1 (Q5).
- Capacity: process-shaped constraints only — existing gates + CodeRabbit loop; front-load connector + streaming (Q6).
- Risks: four tracked — computer-use security model, ai-octane churn, edge streaming, transcript secret leakage — each with a retirement path (Q7).

Does this all look correct before I generate the artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct

## Assumption Confirmation

The artifacts carry these assumptions:

- [assumption] `ai-octane` at 1.0.0 is treated as feasible for the full delivery window; its documented surface (useChat, createChatHook, toolDefinition .server()/.client(), approvals, fetchServerSentEvents) was verified against the registry but not yet exercised in-repo — an early construction-unit spike is the retirement action.
- [assumption] The existing `/api/` proxy path is assumed to support SSE streaming without edge changes; if it does not, an edge-config change becomes a new unit — spike validates this first.
- [assumption] No dedicated team, budget, or deadline constraint applies; delivery proceeds under the standing conductor delegation through the existing PR/CI pipeline.
- [assumption] Constraint RC-3 (no hard regulatory driver) is asserted at ideation depth from the compliance lens; the formal scan in nfr-requirements could surface obligations (e.g. data-residency disclosures) that would be added here rather than argued away.
- [assumption] Risk likelihood/impact ratings are the conductor's qualitative judgment at ideation depth, not measured probabilities — they order work, they are not statistics.

- A. Accept assumptions
- B. Convert to follow-up questions

[Answer]: A. Accept assumptions — conductor-decided under standing instruction (2026-10-27); ratify at PR review.
