# Initiative Brief — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: compiles `intent-capture/` (intent-statement, stakeholder-map),
`market-research/` (competitive-analysis, market-trends, build-vs-buy),
`feasibility/` (feasibility-assessment, constraint-register, raid-log),
`scope-definition/` (scope-document, intent-backlog), `team-formation/`
(team-assessment, skill-matrix, mob-composition), `rough-mockups/`
(wireframes, user-flow). Phase: Ideation → Inception handoff.

## Intent

Give Oxidean users an in-product agentic surface — a `/oxpilot` page hosting
a chat/builder/debugger agent with opt-in computer use against systems the
user connects and controls — on TanStack `ai-octane`, matching the Octane
stack. External agents today cost users the repository/CI context the forge
already holds (`intent-capture/intent-statement.md`).

## Market validation

Three-layer competition (Copilot coding agent GA, GitLab Duo Agent Platform
GA 18.8, Codex cloud; OpenHands + Forgejo/Gitea MCP bridges below). The
**in-forge agent × self-hostable/private** quadrant is empty — no
self-hosted forge ships a native agent, and no vendor offers user-hosted
computer-use targets. Trends favor us: in-product agents are table stakes,
execution consolidates on user-controlled environments, sovereignty demand
grows. (`market-research/`)

## Feasibility & risk

**Feasible.** Verified: `@tanstack/ai-octane@1.0.0` real and maintained,
peer `octane >=0.12.0` satisfied, provider adapters at 1.0.0 (incl. Ollama
for local/self-host). All integration seams exist (RPC dispatch, `/api/`
proxy, session auth, db crate, Octane routes). One novel mechanism — the
outbound-only computer-use connector. Top risks tracked in RAID:
connector security model (R-1), ai-octane churn (R-2), edge streaming
(R-3, early spike), transcript leakage (R-4). (`feasibility/`)

## Scope boundary

In: `/oxpilot` page, streamed repo-aware chat, builder proposing
branch/PR/patch (never autonomous merge), debugger on Actions CI/build
output, per-user session persistence, consented computer-use connector,
feature flag + operator-configured provider + audit + rate limits.
Out: autonomous merge, per-user BYO-key, plugin marketplace, mobile,
multimodal, agent-to-agent, ox-CLI replacement, paid packaging.
(`scope-definition/scope-document.md`)

## Concept

Transcript-first workspace (sessions rail | streamed transcript + inline
tool/action cards | toggled context drawer), `/oxpilot` index with
session list + computer-use targets, consent dialogs for connect and
per-action grants; wireframes passed product-lead review (READY, findings
applied). (`rough-mockups/`)

## Team & delivery

Single conductor lane adopting stage-graph personas; maintainer ratifies at
PR; CodeRabbit loop post-CI. Serial unit-major execution; risk-first
ordering inside the dependency chain (foundation+spike → tools →
builder/debugger → connector → hardening → e2e).
(`team-formation/`, `scope-definition/intent-backlog.md`)

## Recommendation

**GO.** Requirements are grounded in the request, differentiation is real,
feasibility is verified at package level, risks have retirement paths, and
the delivery surface (branch → PR → green CI → CodeRabbit) is defined.

## Assumptions & Open Questions

- [assumption] This brief restates the upstream assumption set
  (intent-capture, market-research, feasibility, scope-definition,
  team-formation, rough-mockups) without re-litigating it — the governing
  record is each stage's `## Assumption Confirmation`; the load-bearing ones
  for inception are: operator-configurable provider, per-user server-side
  sessions, explicit-connect-only targets, deferred builder/debugger action
  surface, `ai-octane` 1.0 stability.
