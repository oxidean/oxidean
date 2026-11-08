# Scope Document — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`,
`feasibility/feasibility-assessment.md`, `feasibility/constraint-register.md`;
answers Q1–Q6 in `scope-definition-questions.md` (conductor-decided under
standing instruction 2026-10-27; ratify at PR review). Delivery-lead
perspective integrated inline.

## Product boundary (in scope — v1)

| Capability | Depth in v1 |
|---|---|
| `/oxpilot` page | Authenticated Octane route; chat workspace with message list, composer, streamed replies, tool/action cards; route-coverage evidence |
| Agentic chat | Multi-turn, repo-aware context (repo metadata, file contents, issue/PR references), streamed via SSE through `/api/`; `useChat`/`createChatHook` on `@tanstack/ai-octane` |
| Builder | Agent proposes changes as a branch + pull request or reviewable patch; never pushes to protected branches autonomously |
| Debugger | Agent inspects Actions CI runs/build logs and surfaces failure analysis + suggested fixes |
| Session persistence | Per-user server-side sessions/transcripts in `oxidean-db` (dialect-agnostic) |
| Computer use (opt-in) | User-connected local/remote target via outbound-only connector; explicit consent, PAT-class credentials, per-action authorization + audit |
| Platform plumbing | Feature flag per deployment, operator-configured LLM provider (`OXIDEAN_*` env, documented), provider adapters incl. local (Ollama-class), audit logging of agent actions, rate limiting |

## Out of scope (v1)

- Autonomous merge or push to protected branches — agent proposes, human merges
- Per-user BYO-LLM-key custody (deployment-scoped provider only)
- Tool plugin marketplace / third-party tool extensions
- Mobile-optimized layout; voice/multimodal input
- Agent-to-agent orchestration
- Replacing the `ox` CLI or external agentic tooling (intent non-goal)
- Paid-add-on packaging / per-token billing
- New deployable service — rides `api` + `web`

## MoSCoW priorities

**Must** (the contract): all capabilities in the boundary table.
**Should**: diff-preview UI polish, queued/follow-up messages, session
list/resume UX. **Could**: per-user provider override, extra tool widgets,
richer interrupt/approval UI beyond the framework default. **Won't (v1)**:
everything in the out-of-scope list.

## Value stream

`/oxpilot` opens → user chats with repo context → agent proposes changes
(builder) or explains failures (debugger) → user reviews a PR/diff →
(optionally) connects a computer-use target → agent acts on the user's own
system under consent + audit. The value compounds: forge context removes the
copy/paste tax external agents impose, and user-connected computer use
extends the agent past the forge's sandbox boundary.

## Constraints honored

`feasibility/constraint-register.md` TC-1…TC-8, OC-1…OC-4, RC-1…RC-4 apply:
Octane-only UI, `/api/` namespace, `oxpilot.*` RPC, db-crate persistence,
PAT-class connector auth, documented env vars, feature flag, PAT/session
rules, trunk+PR delivery, coverage manifests.

## Assumptions & Open Questions

- [assumption] "Full request at reduced depth" is the accepted reading of MVP
  — every capability the request names ships, at v1 depth; any later cut of a
  named capability is a scope change the maintainer ratifies at PR review.
