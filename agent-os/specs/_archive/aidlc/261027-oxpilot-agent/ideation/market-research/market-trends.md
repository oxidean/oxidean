# Market Trends — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`; answers Q3 in
`market-research-questions.md` (conductor-decided under standing instruction
2026-10-27; ratify at PR review).

## Trend 1 — In-product async agents are now table stakes for dev platforms

Within ~12 months every major code platform shipped an embedded async agent:

- GitHub Copilot coding agent reached **general availability for all paid
  Copilot subscribers** ([changelog 2025-09-25](https://github.blog/changelog/2025-09-25-copilot-coding-agent-is-now-generally-available/)).
- GitLab Duo Agent Platform reached GA in **GitLab 18.8** ([docs](https://docs.gitlab.com/user/duo_agent_platform/)).
- OpenAI Codex runs engineering tasks in managed cloud sandboxes and surfaces
  them across ChatGPT, IDE, and terminal ([openai.com/codex](https://openai.com/codex/)).
- Google Jules, Devin, and IDE-side agents (Cursor, Claude Code) fill out the
  same direction: delegation to an agent is becoming a first-class surface.

**Implication for `/oxpilot`:** an in-product agent is no longer a novelty —
it is an expected surface. The cost of not shipping is feature-parity drift
against forges users compare us to.

## Trend 2 — Agent execution consolidates on isolated, auditable, user-controlled environments

Every serious agent product now runs work in a bounded environment rather
than on the host machine:

- Copilot coding agent works in an Actions-powered ephemeral dev environment
  ([docs](https://docs.github.com/en/copilot/concepts/coding-agent/coding-agent)).
- Codex cloud runs each task in an isolated sandbox with internet disabled by
  default ([system-card addendum](https://openai.com/index/o3-o4-mini-codex-system-card-addendum/)).
- OpenHands separates the agent server from its sandbox backends
  ([docs](https://docs.openhands.dev/overview/introduction)).
- Anthropic's computer-use API is a *client* toolset — the vendor supplies
  the tool contract, the application runs every action in an environment it
  controls ([platform docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/computer-use-tool)).

**Implication for `/oxpilot`:** the market has normalized "the agent acts
inside an environment, not on the open host." Our computerUse differentiator
extends this one step the vendors haven't taken — the *user chooses which
system is the environment* (their local machine or a remote host), under an
explicit, consented connection. Auditing, consent, and isolation boundaries
are therefore table-stakes design inputs, not optional hardening.

## Trend 3 — Sovereignty and privacy demand favors self-hostable AI plumbing

- GitLab offers Duo with self-hosted models for self-managed instances
  ([docs](https://docs.gitlab.com/user/duo_agent_platform/)).
- OpenHands' growth (~92k GitHub stars, enterprise control-plane pitch) is
  driven substantially by "keep models, data, and access under your control"
  ([openhands.dev](https://www.openhands.dev/)).
- The Forgejo/Gitea ecosystem's MCP bridges exist because that audience
  refuses cloud-only agent products.

**Implication for `/oxpilot`:** self-host parity and a provider-configurable
LLM (including local models via Ollama-class adapters) are not niche
requirements — they are the segment where our differentiation is strongest.
The feature must ship optional and configurable, never a forced external
dependency (consistent with the intent statement's self-host requirement).

## Trend 4 (watch) — Agent framework commoditization

Provider-neutral agent SDKs (TanStack AI among them) now package the
streaming/tool/approval primitives that used to be bespoke. This lowers the
cost of building a first-party agent and raises the cost of embedding a
heavyweight third-party runtime. It also means framework choices are
reversible — our adopted `ai-octane` can be swapped behind the forge-tool
layer if it stalls, which the build-vs-buy assessment carries as a fallback.

## Assumptions & Open Questions

- [assumption] Trend claims are sourced from vendor documentation and public
  product pages cited inline; adoption/revenue data behind the vendors'
  claims was not independently verified — treat "table stakes" as a
  positioning judgment, not a measured market share.
- [assumption] No regulatory/driver requirement (e.g. EU AI Act agent-specific
  duties) was identified for this feature at ideation depth; a formal
  compliance scan is deferred to nfr-requirements if the feature's
  data-handling warrants it.
