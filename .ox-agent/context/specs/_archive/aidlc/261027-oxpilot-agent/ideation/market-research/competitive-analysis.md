# Competitive Analysis — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`; answers Q1–Q3 in
`market-research-questions.md` (all conductor-decided under standing
instruction 2026-10-27; ratify at PR review).

## Competitive Set

Three layers compete for the Oxidean user's agentic-forge attention.

### Layer 1 — Forge-embedded agents

| Competitor | What it ships | Where it runs | Pricing posture |
|---|---|---|---|
| GitHub Copilot coding agent | Async issue→PR delegation, PR-mention iteration, agents panel; Actions-powered ephemeral dev environment | GitHub cloud only | Bundled in paid Copilot tiers ([GA 2025-09-25](https://github.blog/changelog/2025-09-25-copilot-coding-agent-is-now-generally-available/)) |
| GitHub Copilot cloud agent | Research/plan/iterate sessions on github.com, diff review before PR | GitHub cloud only | Same ([docs](https://docs.github.com/en/copilot/how-tos/copilot-on-github/use-copilot-agents/research-plan-iterate)) |
| GitLab Duo Agent Platform | Agentic Chat on nearly every page, async flows, foundational + custom flows, self-hosted-model option | GitLab cloud; self-managed behind `self_hosted_agent_platform` (GA 18.8) | Paid Duo tiers ([docs](https://docs.gitlab.com/user/duo_agent_platform/), [GA post](https://about.gitlab.com/blog/gitlab-duo-agent-platform-is-generally-available/)) |

### Layer 2 — Standalone cloud agents

| Competitor | What it ships | Where it runs |
|---|---|---|
| OpenAI Codex cloud | Parallel tasks in isolated managed sandboxes per repo, browser+computer tools, PR review bot | OpenAI-managed cloud ([openai.com/codex](https://openai.com/codex/)) |
| Google Jules | Async coding tasks in cloud VMs, GitHub-connected | Google cloud |
| Devin (Cognition) | Autonomous SWE agent with its own IDE/shell/browser env | Vendor cloud |
| IDE-side agents (Cursor, Claude Code agent mode) | Interactive/local agent loops | User's machine |

### Layer 3 — Open-source agent platforms & bridges

| Competitor | What it ships | Relevance |
|---|---|---|
| [OpenHands](https://github.com/OpenHands/OpenHands/) (~92k stars, MIT) | Agent Canvas UI + Agent Server/SDK (REST/WS), sandbox backends, multi-agent support | The credible self-hostable agent platform; not forge-embedded |
| Forgejo/Gitea MCP bridges ([forgejo-mcp](https://github.com/Sqcows/forgejo-mcp), [gitx-mcp](https://github.com/0xErgod/gitx-mcp)) | MCP tool servers letting external agents touch Forgejo/Gitea repos/issues/PRs | Evidence the self-hosted forge audience wants agent access but has no native option |
| External agent services for Gitea ([FragonForge](https://fragonforge.com/gitea/), Archon adapter) | Webhook-triggered agents that open PRs on Gitea | Bolt-on pattern: agent lives outside the forge |

**Key gap:** no self-hostable forge ships a native in-product agent. Forgejo
and Gitea have zero built-in agent surface — the community fills it with MCP
bridges and external services. GitLab Duo is the closest embedded competitor
but is a paid add-on with a heavyweight self-managed install path.

## Feature Comparison vs. Oxidean Target

| Capability | Copilot coding agent | Duo Agent Platform | Codex cloud | OpenHands | Oxidean `/oxpilot` (target) |
|---|---|---|---|---|---|
| Repo-aware multi-turn chat | Strong | Strong | Adequate | Strong | Target |
| Propose changes as reviewable PRs | Strong | Strong | Strong | Adequate | Target |
| CI/build-log awareness | Strong (Actions) | Strong (pipelines) | Adequate | Weak | Target (native Actions context) |
| Execution on user-connected system | Absent | Absent | Absent | Adequate (backend choice) | **Differentiator** |
| Self-host parity | Absent | Adequate (heavy, flag-gated) | Absent | Strong | **Differentiator** |
| Operator-configurable LLM (incl. local) | Absent | Adequate | Absent | Strong | **Differentiator** |
| Open-source trust/auditability | Absent | Absent | Absent | Strong | **Differentiator** |

## Positioning

The underserved quadrant is **in-forge agent × self-hostable/private
deployment**. Copilot and Codex are cloud-only. GitLab Duo self-hosting exists
but is an add-on SKU with substantial install weight. OpenHands self-hosts the
*agent* but sits outside the forge — users still context-switch, and forge
context (issues, PRs, CI) reaches it only through MCP-style bridging.

Oxidean `/oxpilot` occupies the empty quadrant: a forge-native agent surface
that can also act on systems the user owns (local dev box, remote build host)
rather than only a vendor sandbox — matching the market's direction of
isolated, auditable, user-controlled execution.

## Threats & Watch Items

- GitHub could ship an agent surface for GHES; Copilot already has the mindshare.
- GitLab Duo's self-managed path may mature and close the gap.
- OpenHands' Agent Canvas could deepen forge integrations (it already drives
  Gitea via webhooks via third parties).
- Computer-use model tooling is young (Anthropic's `computer_toolset` is a
  beta tool contract) — API churn risk lands on our connector layer.

## Assumptions & Open Questions

- [assumption] Competitor capability rows reflect public vendor documentation
  as of 2026-10-27; no hands-on competitive evaluation was performed — feature
  parity claims should be re-verified before any external positioning copy.
- [assumption] The self-hosted-forge gap (no native agent on Forgejo/Gitea) is
  assumed stable through the delivery window; a surprise native-agent release
  by either would compress the differentiation window.
