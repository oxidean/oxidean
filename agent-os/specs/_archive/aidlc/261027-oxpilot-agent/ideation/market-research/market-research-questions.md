# Market Research Questions — `/oxpilot` Agentic Chat / Builder / Debugger

Upstream input: `intent-capture/intent-statement.md` (this stage consumes it).

## Sources

- [desc] Initial description: "Fully self-driven I want you to plan and build a full setup of a agentic chat/builder/debuger that has its own /oxpilot page, has computerUse (that allows the end-user to connect the agent to their local system or a remote system to their own liking and configuration). For the framework I want to use https://github.com/TanStack/ai/tree/main/packages/ai-octane and please go for best effort around standards, UI design/UX, security/privacy and usability. I want you to self-drive this until we have a new branch with a PR against main with green CI. Once CI is green I want you to trigger coderabbit AI for review, poll for its response, fix them and do this until coderabbit doesnt find anything anymore. To trigger a review, check the comment it posts or just comment `@coderabbitai review` yourself"
- [scope] Workflow-selected scope: `feature`.

## Q1. Which products compete for the user's agentic-forge attention?

Context: the intent statement frames Oxidean users leaving the forge for external agentic tools (intent-capture/`intent-statement.md`, Problem Statement). The competitive set spans forge-embedded agents, standalone cloud agents, and open-source agent platforms.

- A. Three-layer set — forge-embedded (GitHub Copilot coding agent, GitLab Duo Agent Platform), standalone cloud agents (OpenAI Codex cloud, Google Jules, Devin, Cursor/Claude Code agents), and open-source agent platforms (OpenHands + MCP bridges like forgejo-mcp/gitx-mcp for self-hosted forges). Self-hosted forges (Forgejo/Gitea) have no native agent — only external MCP tooling.
- B. Only the big two forges matter (GitHub Copilot, GitLab Duo); standalone and open-source agents are different markets.
- C. Only IDE-side assistants (Cursor, Claude Code) — the comparison set is developer tooling, not forges.
- D. No competitive analysis needed — this is an internal feature.
- X. Other (please specify)

[Answer]: A — three-layer competitive set: forge-embedded agents (Copilot coding agent, GitLab Duo Agent Platform), standalone cloud agents (Codex, Jules, Devin, Cursor/Claude), and open-source platforms (OpenHands + Forgejo/Gitea MCP bridges). Self-hosted forges ship no native agent today. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. What is table-stakes vs. differentiating for an in-forge agent?

Context: Copilot coding agent (GA Sep 2025) offers issue→PR delegation with Actions-powered dev environments; GitLab Duo Agent Platform (GA 18.8) offers agentic chat across the web UI plus async flows; Codex cloud offers parallel tasks in isolated managed sandboxes.

- A. Table-stakes: multi-turn repo-aware chat, propose changes as reviewable PRs/branch diffs, CI/build-log awareness, session persistence, human review before merge. Differentiators for Oxidean: opt-in computerUse against *user-connected* local/remote systems (not a vendor sandbox), self-host parity (code + agent never leave operator infra), provider-configurable LLM (incl. local models), and open-source/community trust.
- B. Table-stakes: full autonomous issue resolution at Copilot parity. Differentiators: none — match features only.
- C. Table-stakes: a chat box that can answer repo questions. Differentiators: computerUse alone carries the feature.
- D. No differentiation analysis needed.
- X. Other (please specify)

[Answer]: A — table-stakes: repo-aware multi-turn chat, reviewable PRs/diffs, CI/build-log awareness, session persistence, human review before merge. Differentiators: user-connected computerUse (not a vendor sandbox), self-host parity, provider-configurable LLM incl. local models, open-source trust. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Which market trends and shifts matter most for this feature?

Context: 2025-26 saw every major forge/devtool ship an in-product async agent; agent execution moved to isolated sandboxes; and privacy/data-residency pressure keeps pushing teams toward self-hostable tooling.

- A. Three trends — (1) in-product async agents are now table stakes for dev platforms (Copilot GA, Duo GA, Codex, Jules); (2) execution environments are consolidating on isolated, auditable sandboxes under user control; (3) sovereignty/privacy demand favors self-hostable AI plumbing (GitLab self-hosted models, OpenHands growth) — Oxidean's self-host parity plus user-connected computerUse aligns with all three.
- B. Only trend (1) matters — in-product agent parity is the whole story; computer use and self-hosting are niche.
- C. The dominant trend is IDE-side agents; forge surfaces are secondary.
- D. No trend analysis needed.
- X. Other (please specify)

[Answer]: A — three trends: (1) in-product async agents are table stakes for dev platforms; (2) agent execution consolidates on isolated, auditable, user-controlled environments; (3) sovereignty/privacy demand favors self-hostable AI plumbing. Oxidean's self-host parity + user-connected computerUse align with all three. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Build-vs-buy-vs-partner for the agent framework itself?

Context: the request names TanStack `ai-octane` as the desired framework. Alternatives: OpenHands Agent Server/SDK (REST+WS agent backend, MIT), vendor agent APIs (Copilot/Codex SDKs are not embeddable), or a bespoke agent loop on raw provider SDKs.

- A. Adopt `ai-octane` as directed — it is Octane-native (uncompiled `.tsrx`, `useChat`/`createChatHook`, provider adapters incl. OpenAI/Anthropic/Ollama/OpenRouter, isomorphic server/client tools with approvals) and matches the mandated stack; build the forge-domain tools, session store, and computer-use connector ourselves. OpenHands Agent Server is the fallback only if `ai-octane` proves immature — embedding it adds a Python service runtime to a Bun/Rust stack.
- B. Embed OpenHands Agent Server as the agent backend — use its mature tool/runtime set and treat `ai-octane` as UI-only.
- C. Build a bespoke agent loop on provider SDKs — no framework dependency.
- D. Buy/partner with a vendor agent product (e.g. embed a hosted agent).
- X. Other (please specify)

[Answer]: A — adopt `ai-octane` as directed (Octane-native `.tsrx`, useChat/createChatHook, provider adapters incl. Ollama/OpenRouter, isomorphic tools with approvals); build forge tools, session store, and the computer-use connector ourselves. OpenHands Agent Server is the fallback if `ai-octane` proves immature. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Build-vs-buy for computerUse connectivity?

Context: the request's differentiator is letting the end user connect the agent to *their own* local or remote system. Options: Anthropic's computer-use toolset (model-side tool contract, we implement the driver), OpenAI CUA/Operator (hosted only), or a custom connector protocol.

- A. Build a first-party computer-use connector protocol (agent ↔ user-side connector over an authenticated, consented channel), and expose it to models through provider-appropriate tools (e.g. Anthropic's computer-use toolset when the operator picks an Anthropic adapter). No vendor computer-use product offers user-hosted targets — they all assume vendor-managed sandboxes.
- B. Use a hosted computer-use product (OpenAI Operator/CUA) — accept vendor-managed targets only.
- C. Skip computerUse in v1 — ship chat/builder/debugger first.
- D. No analysis needed.
- X. Other (please specify)

[Answer]: A — build a first-party computer-use connector protocol (agent ↔ user-side connector over an authenticated, consented channel), surfaced to models via provider-appropriate tools such as Anthropic's computer-use toolset. No vendor computer-use product supports user-hosted targets. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. What is the addressable audience for this feature?

Context: Oxidean ships one product for cloud + self-host; the feature's audience is existing Oxidean users plus prospective adopters for whom a built-in agent influences forge choice.

- A. Primary: existing Oxidean users (cloud + self-host) who currently use external agents — the feature removes context-switching. Secondary: forge evaluators for whom in-product AI is a selection criterion, especially privacy-sensitive self-host adopters — the segment where Oxidean's differentiation is strongest. No TAM sizing; this is a feature-level decision inside the existing product, not a new revenue line.
- B. The feature targets a new paid tier — size the market commercially.
- C. Only existing self-host operators matter.
- D. No audience analysis needed.
- X. Other (please specify)

[Answer]: A — primary: existing Oxidean users (cloud + self-host) using external agents today; secondary: forge evaluators for whom in-product AI is a selection criterion, strongest in privacy-sensitive self-host. No TAM sizing — a feature-level decision inside the existing product. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q7. What is the pricing/packaging posture?

Context: Oxidean is open source with cloud hosting; GitLab sells Duo as a paid add-on, GitHub gates Copilot tiers, OpenHands is free/OSS with paid cloud.

- A. Ship as a core platform feature on cloud and self-host; usage cost flows through the operator's own provider configuration (self-host = their keys/local models; cloud = operator-configured provider), matching the "operator-configurable LLM" assumption. Feature-flag/optional per the self-host requirement — no forced external dependency.
- B. Paid add-on on cloud, free on self-host.
- C. Free-tier-only teaser that upsells external agents.
- D. No packaging decision needed.
- X. Other (please specify)

[Answer]: A — core platform feature on cloud and self-host; usage cost flows through operator-configured providers (own keys/local models on self-host). Optional/feature-flagged so no forced external dependency. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Competition: three layers — forge-embedded (Copilot coding agent, GitLab Duo Agent Platform), standalone cloud agents (Codex, Jules, Devin), open-source platforms (OpenHands, forge MCP bridges); self-hosted forges ship no native agent (Q1).
- Table-stakes vs differentiation: repo-aware chat, reviewable PRs, CI awareness, session persistence, human review are table stakes; user-connected computerUse, self-host parity, provider-configurable LLM, and OSS trust differentiate (Q2).
- Trends: in-product async agents now table stakes; execution consolidates on isolated user-controlled environments; sovereignty demand favors self-hostable AI (Q3).
- Framework: adopt TanStack `ai-octane` as directed; OpenHands Agent Server is the fallback if it proves immature (Q4).
- ComputerUse: build a first-party connector protocol; no vendor product supports user-hosted targets (Q5).
- Audience: existing Oxidean users first, forge evaluators second — strongest in privacy-sensitive self-host (Q6).
- Packaging: core platform feature on cloud + self-host; operator-configured providers; optional/flagged (Q7).

Does this all look correct before I generate the artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct

## Assumption Confirmation

The artifacts carry these assumptions:

- [assumption] Competitor capability rows reflect public vendor documentation as of 2026-10-27; no hands-on competitive evaluation was performed — feature parity claims should be re-verified before any external positioning copy.
- [assumption] The self-hosted-forge gap (no native agent on Forgejo/Gitea) is assumed stable through the delivery window; a surprise native-agent release by either would compress the differentiation window.
- [assumption] Trend claims are sourced from vendor documentation and public product pages cited inline; adoption/revenue data behind the vendors' claims was not independently verified — treat "table stakes" as a positioning judgment, not a measured market share.
- [assumption] No regulatory/driver requirement (e.g. EU AI Act agent-specific duties) was identified for this feature at ideation depth; a formal compliance scan is deferred to nfr-requirements if the feature's data-handling warrants it.
- [assumption] `ai-octane` is assumed mature enough to build on — its documented surface (useChat, createChatHook UI factory, toolDefinition with .server()/.client(), approvals, fetchServerSentEvents transport) covers the required primitives, but version stability is unverified and carried as a feasibility checkpoint.
- [assumption] No vendor/partner option for user-hosted computer-use targets is assumed to appear during the delivery window; if one emerges, the build-vs-buy on Decision B should be revisited.

- A. Accept assumptions
- B. Convert to follow-up questions

[Answer]: A. Accept assumptions — conductor-decided under standing instruction (2026-10-27); ratify at PR review.
