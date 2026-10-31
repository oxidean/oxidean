# Build vs. Buy — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`; answers Q4–Q7 in
`market-research-questions.md` (conductor-decided under standing instruction
2026-10-27; ratify at PR review).

Two separable decisions: (A) the agent framework/UI plumbing, and (B) the
computerUse connectivity. Packaging posture follows at the end.

## Decision A — Agent framework

| Factor | Build (bespoke on provider SDKs) | Adopt `ai-octane` | Embed OpenHands Agent Server | Buy (vendor agent) |
|---|---|---|---|---|
| Stack fit | Native but re-builds streaming/tools/approvals | Octane-native `.tsrx`, `useChat`/`createChatHook` | Python runtime into a Bun/Rust stack | None embeddable |
| Provider flexibility | Full, hand-rolled | Adapters incl. OpenAI/Anthropic/Gemini/Ollama/OpenRouter | Provider-agnostic via its config | Vendor-locked |
| Computer-use tool contracts | Hand-rolled | Provider tool adapters | Its own tool set | n/a |
| Forge-domain tools | Build | Build (isomorphic server/client tools w/ approvals) | Bridge its tools to forge RPC | n/a |
| Maintenance burden | Highest | Framework-tracked by TanStack | Second runtime to operate | Vendor roadmap |
| Time to surface | Slowest | Fastest credible path | Medium (integration weight) | n/a |

**Decision: adopt `@tanstack/ai-octane`** — the directed choice, and the
evaluation supports it independently: it is the only option native to the
mandated Octane stack, it ships the exact primitives this surface needs
(streaming chat hooks, typed isomorphic tools, approval interrupts, SSE
transport via `fetchServerSentEvents`), and it keeps the provider layer
pluggable — which the operator-configurable-LLM requirement depends on.

**Fallback:** if `ai-octane` proves immature during feasibility/construction,
OpenHands Agent Server (MIT, REST+WS) is the credible alternative backend —
accepted cost: a second service runtime and a bridge to forge RPC.
Feasibility stage should carry an explicit maturity check on `ai-octane`
(version stability, toolset completeness, approvals flow).

## Decision B — Computer-use connectivity

| Factor | Build first-party connector | Buy/hosted computer-use (e.g. OpenAI Operator/CUA) |
|---|---|---|
| User-hosted targets | Yes — the requirement | No — vendor-managed sandboxes only |
| Self-host parity | Full | Broken (forces cloud dependency) |
| Privacy posture | User's system, user's consent, operator's audit | Data flows through vendor |
| Model support | Provider-appropriate tools (e.g. Anthropic `computer_toolset_20260801` when an Anthropic adapter is configured) | OpenAI models only |

**Decision: build.** No buy option exists for the actual requirement —
"the end user connects the agent to their local or remote system" is a
capability no vendor sells, because every vendor's computer-use product
assumes the vendor owns the environment. The build is a connector protocol
(authenticated, consented agent↔user-connector channel) plus the model-side
tool surface providers already define. This is the feature's core
differentiator, so the build cost is justified.

## Packaging posture

Ship as a **core platform feature** on cloud and self-host — not a paid
add-on:

- Matches the request's framing (a product capability, not a SKU).
- Self-host requirement demands it be optional/feature-flagged with no forced
  external dependency — usage cost flows through the operator's own provider
  configuration (their keys or local models).
- GitLab's Duo-as-add-on model is a revenue choice, not a technical
  constraint; Oxidean's open-source trust positioning favors core inclusion.

## Risks carried forward

| Risk | Where addressed |
|---|---|
| `ai-octane` maturity/API churn (package publishes uncompiled `.tsrx`; young ecosystem) | feasibility — explicit maturity checkpoint |
| Computer-use connector is novel — consent, authn/z, audit model must be designed carefully | requirements-analysis, domain-design, nfr-security work |
| "Optional feature flag" adds config surface to every deployment | requirements-analysis, infrastructure-design |

## Assumptions & Open Questions

- [assumption] `ai-octane` is assumed mature enough to build on — its
  documented surface (useChat, createChatHook UI factory, toolDefinition with
  .server()/.client(), approvals, fetchServerSentEvents transport) covers the
  required primitives, but version stability is unverified and carried as a
  feasibility checkpoint.
- [assumption] No vendor/partner option for user-hosted computer-use targets
  is assumed to appear during the delivery window; if one emerges, the
  build-vs-buy on Decision B should be revisited.
