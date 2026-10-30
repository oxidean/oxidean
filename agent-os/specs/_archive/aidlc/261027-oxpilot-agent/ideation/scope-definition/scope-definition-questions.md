# Scope Definition Questions — `/oxpilot` Agentic Chat / Builder / Debugger

Upstream inputs: `intent-capture/intent-statement.md`,
`feasibility/feasibility-assessment.md`, `feasibility/constraint-register.md`
(this stage consumes all three).

## Sources

- [desc] Initial description: "Fully self-driven I want you to plan and build a full setup of a agentic chat/builder/debuger that has its own /oxpilot page, has computerUse (that allows the end-user to connect the agent to their local system or a remote system to their own liking and configuration). For the framework I want to use https://github.com/TanStack/ai/tree/main/packages/ai-octane and please go for best effort around standards, UI design/UX, security/privacy and usability. I want you to self-drive this until we have a new branch with a PR against main with green CI. Once CI is green I want you to trigger coderabbit AI for review, poll for its response, fix them and do this until coderabbit doesnt find anything anymore. To trigger a review, check the comment it posts or just comment `@coderabbitai review` yourself"
- [scope] Workflow-selected scope: `feature`.

## Q1. What is the minimum viable scope that delivers value?

Context: the request names three capabilities — agentic chat, builder/debugger actions, and computerUse — plus the `/oxpilot` page itself. Feasibility says all are buildable on existing seams.

- A. MVP = `/oxpilot` page + authenticated multi-turn agentic chat with repo-aware context + builder action surface (propose changes as branch/PR) + debugger surface (inspect CI/build failures) + session persistence + the opt-in computer-use connector — i.e. the full request, because every named capability is core to the ask. Cut line lives inside each capability's depth (e.g. one provider-adapter set, one connector transport), not in dropping a named capability.
- B. MVP = chat only; builder/debugger and computerUse are v2.
- C. MVP = page skeleton behind a flag with no agent loop.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — MVP is the full request at reduced depth per capability: /oxpilot page + repo-aware multi-turn streamed chat + builder (branch/PR proposals) + debugger (Actions CI/build inspection) + session persistence + opt-in computer-use connector. Cut depth inside capabilities, not named capabilities. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. MoSCoW priority of capabilities?

Context: cutting depth inside capabilities is available; the request's named items are presumed Must.

- A. Must: `/oxpilot` page, agentic chat (repo-aware, multi-turn, streamed), builder surface proposing reviewable changes (branch+PR, not silent direct-push), debugger reading Actions CI/build output, per-user session persistence, computer-use connector (explicit-connect, consented, PAT-class creds), feature flag + provider config + audit log, security/privacy controls. Should: diff preview UI polish, queued/follow-up messages, session listing/resume UX. Could: per-user provider override, additional tool widgets, interrupt/approval UI beyond the framework default. Won't (v1): autonomous merge, marketplace/tool plugins, mobile layout, replacing `ox` CLI.
- B. All named items plus per-user keys and multi-provider failover are Must.
- C. Only chat is Must; everything else is Could.
- D. Not prioritized.
- X. Other (please specify)

[Answer]: A — Must: page, streamed repo-aware chat, builder proposing reviewable changes, debugger on CI/build output, session persistence, consented computer-use connector with PAT-class creds, flag+provider config+audit log+security controls. Should: diff-preview polish, queued messages, session list/resume UX. Could: per-user provider override, extra tool widgets, richer interrupt UI. Won't v1: autonomous merge, plugin marketplace, mobile layout, replacing ox CLI. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. What is explicitly OUT of scope?

Context: the intent statement's non-goal is "does not replace the `ox` CLI or external agentic tooling"; feasibility excluded per-user key custody and paid-SKU packaging.

- A. Out for v1: autonomous merge/push to protected branches (agent proposes, human merges), per-user BYO-LLM-key custody, tool plugin marketplace, mobile-optimized layout, voice/multimodal input, agent-to-agent orchestration, replacing `ox` CLI or external agents, any paid-add-on packaging change.
- B. Nothing is out — the request is open-ended.
- C. Computer-use is out (too risky for v1).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — out for v1: autonomous merge/push to protected branches, per-user BYO-key custody, plugin marketplace, mobile layout, voice/multimodal, agent-to-agent orchestration, ox CLI replacement, paid-add-on packaging. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. What are the capability dependencies?

Context: feasibility sequenced risk-first with the streaming spike early.

- A. Foundation chain: session store + provider config → chat endpoint + `/oxpilot` page + SSE spike → forge tools (repo read, branch/patch proposal, CI log read) → builder+debugger UX → computer-use connector (consent, authn, channel) → hardening (flag, audit, rate limits, docs) → e2e coverage. Computer-use depends on the tool framework and session model; debugger depends on Actions read APIs (verify coverage exists — RAID D-4).
- B. All capabilities are independent — order by convenience.
- C. Computer-use first since it's the differentiator.
- D. Not mapped.
- X. Other (please specify)

[Answer]: A — foundation chain: session store + provider config → chat endpoint + /oxpilot page + SSE spike → forge tools (repo read, branch/patch, CI log read) → builder+debugger UX → computer-use connector → hardening → e2e coverage. Debugger depends on Actions read API coverage (RAID D-4 — verify at requirements-analysis). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. What is the sequencing preference?

Context: RAID puts computer-use security and edge-streaming as the top risks; delivery is a single conductor-driven flow.

- A. Risk-first inside a dependency-valid order: land the foundation (session+provider+page) then spike streaming (R-3) and the connector security model (R-1) early — before bulk UI polish — so architectural surprises surface while cheap. Then builder/debugger depth, then hardening and coverage.
- B. Value-first: the most demo-visible pieces (chat UI) get fully polished first.
- C. Dependency-first strictly: strict linear order regardless of risk.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — risk-first inside a dependency-valid order: foundation, then streaming spike (R-3) and connector security model (R-1) early, then builder/debugger depth, then hardening and coverage. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q6. Are there hard deadlines tied to specific capabilities?

Context: the request sets no date; the acceptance criterion is process-shaped (branch → PR → green CI → clean CodeRabbit).

- A. No hard deadlines — quality-gated delivery: done means the PR is merge-ready with green CI and a clean CodeRabbit pass, not a date.
- B. A fixed release date applies.
- C. Deadlines exist only for the page skeleton.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — no hard deadlines; quality-gated delivery: merge-ready PR + green CI + clean CodeRabbit pass. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- MVP: the full request — `/oxpilot` page, streamed repo-aware chat, builder (branch/PR proposals), debugger (CI/build inspection), session persistence, opt-in computer-use connector; depth cut inside each capability (Q1).
- MoSCoW: the named capabilities + flag/config/audit/security controls are Must; polish items Should; per-user provider override Could; autonomous merge/marketplace/mobile/ox-replacement Won't (Q2).
- Out of scope v1: autonomous merge/push, per-user key custody, plugin marketplace, mobile, multimodal, agent orchestration, CLI replacement, paid packaging (Q3).
- Dependencies: session+provider → chat endpoint + page + SSE spike → forge tools → builder/debugger UX → connector → hardening → e2e (Q4).
- Sequencing: risk-first inside dependency-valid order — streaming spike and connector security model early (Q5).
- Deadlines: none; quality gate is the delivery criterion (Q6).

Does this all look correct before I generate the artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct

## Assumption Confirmation

The artifacts carry these assumptions:

- [assumption] "Full request at reduced depth" is the accepted reading of MVP — every capability the request names ships, at v1 depth; any later cut of a named capability is a scope change the maintainer ratifies at PR review.
- [assumption] Proto-unit boundaries are provisional — delivery-planning may merge P-1+P-2 (foundation+page spike) and split P-6 (connector protocol vs connector client); the ordering, not the count, is the committed part.

- A. Accept assumptions
- B. Convert to follow-up questions

[Answer]: A. Accept assumptions — conductor-decided under standing instruction (2026-10-27); ratify at PR review.
