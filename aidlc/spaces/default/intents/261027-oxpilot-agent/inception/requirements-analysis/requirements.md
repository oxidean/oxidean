# Requirements — `/oxpilot` Agentic Chat / Builder / Debugger

Status: confirmed against `requirements-analysis-questions.md` Q1–Q8.

Upstream inputs: `ideation/intent-capture/intent-statement.md` (problem,
success metrics, delivery bar), `ideation/scope-definition/scope-document.md`
(boundary + MoSCoW), `codekb/business-overview.md` + `codekb/architecture.md`
+ `codekb/code-structure.md` (verified seams), and
`inception/practices-discovery/team-practices.md` (affirmed ways of working,
mandated rules). Confirmed answers: `requirements-analysis-questions.md` Q1–Q8.

## Intent Analysis

The user wants a first-class agentic surface inside Oxidean: a `/oxpilot`
page hosting a multi-turn, repository-aware chat that can propose code
changes (builder), inspect CI failures (debugger), and — only when the user
explicitly connects and consents — act on their own local or remote system
(computer use). Everything the agent changes is reviewable; nothing merges
or pushes autonomously. The feature is operator-optional (feature flag +
operator-configured provider) and must work on cloud and self-host,
including against local models.

Delivery contract (verbatim from the request): a new branch, a PR against
`main`, green CI, then a CodeRabbit review loop (`@coderabbitai review`)
driven to zero findings.

## Functional Requirements

### FR1 — `/oxpilot` session index

FR1.1 The app exposes a `/oxpilot` top-level route, registered through the
existing shell+route pair pattern (`SHELLS` entry, Astro page shell,
`.tsrx` route, `route-coverage.manifest.ts` row) plus the route-sync
triangle's remaining legs: a `RESERVED_TOP_SEGMENTS` entry in
`route-params.ts` (without it the segment misbinds as `/{owner}`) and the
generated `routeTree` sync.
FR1.2 The index lists the signed-in user's agent sessions (newest first)
with title, repository, status, and last activity.
FR1.3 A "New session" surface offers repository select, branch select, goal
prompt, and optional computer-use target — matching rough-mockup Screen 2b.
FR1.4 The index hosts the computer-use target manager (list, connect,
disconnect, revoke) linked from `/settings/ai`; `/settings/ai` remains the
external-agent integration surface.
FR1.5 When the feature is disabled or unconfigured, the index renders an
operator-disable state naming the `OXIDEAN_OXPILOT_*` env vars and pointing
at `docs/CONFIGURATION.md`.
FR1.6 Any authenticated user may use `/oxpilot` when the feature flag and
provider are configured; there is no additional role gate in v1.
FR1.7 Users can delete their own sessions and transcripts; deletion is
per-user scoped and removes the persisted rows.

### FR2 — Session workspace and streaming chat

FR2.1 `/oxpilot/<session>` renders the three-pane workspace: sessions rail
(with connection status + collapse), streamed transcript + docked composer,
toggled context drawer — matching rough-mockup Screen 2.
FR2.2 The agent replies with streamed output (SSE over `/api/`, per the
verified `stream_job_log` precedent); each user turn gets an assistant reply
or a visible error without page reload.
FR2.3 The chat is repository-aware: the session binds a repository + branch
and the agent is served repository context (tree, files, issues, PRs, CI
status) through typed RPC tools.
FR2.4 Multi-turn conversation state persists server-side per user; a
session can be reopened after navigation or a new browser session.
FR2.5 Message boundaries expose `aria-live="polite"` updates; the composer
is a labelled form; all controls are real buttons in tab order.

### FR3 — Builder

FR3.1 On request, the agent produces a reviewable diff/patch for the bound
repository and presents it in-session (per-hunk or file view).
FR3.2 On explicit user action, the agent materializes an approved proposal
as a new branch plus pull request via existing repo/PR RPC, then links the
PR in-session.
FR3.3 The agent never pushes, merges, or modifies `main` without the user's
explicit per-action approval; there is no auto-merge path.

### FR4 — Debugger

FR4.1 The agent can list Actions runs/jobs and fetch job logs for the bound
repository (read-only CI surface).
FR4.2 On request it summarizes a failure and proposes a fix through the
same reviewable-proposal path as FR3.
FR4.3 No rerun/cancel/approval control in v1.

### FR5 — Computer use (opt-in)

FR5.1 Users register a computer-use target by running a first-party
connector on a system they own (local or remote); the connector dials out
to the forge — no inbound listener is required on the user's system beyond
their own connector process.
FR5.2 Connecting is explicit and consented: the UI shows what the target
grants before activation; grants are per-action or per-session with no
cross-session "always allow" in v1.
FR5.3 Connector capability bound: shell command execution and filesystem
read/write only; GUI/screenshot control is out of scope for v1.
FR5.4 Target credentials are stored encrypted at rest (AES-256-GCM via the
env-key chain under a named `OXIDEAN_OXPILOT_*` var); verifier-class
material is stored hashed. Agent-reachable surfaces never return stored
secret material (mandated rule).
FR5.5 Every connector action is audited (who/what/when/result) into
`audit_events`; audited action detail is secrets-redacted before write —
command text or payloads must never carry secret material into the audit
store.
FR5.6 Users can disconnect/revoke a target at any time; revocation is
immediate and terminates pending grants.

### FR6 — Agent backend

FR6.1 Agent sessions run through the TanStack `ai-octane` chat/runtime
layer (`useChat`-family on the client; server-side tool execution on the
API tier).
FR6.2 The control plane is a new `oxpilot.*` RPC family in a new
`oxidean-api-oxpilot` domain crate (PROCEDURES + dispatch arm + workspace
member + versioning component + changelog — mandated registration rule);
procedures are `SessionOnly`-classified for PATs in v1 with a pinned
closed-set classification test.
FR6.3 Chat streaming uses a dedicated SSE route under `/api/` (verified
`stream_job_log` precedent — snapshot then wake-append); the web tier's
`text/event-stream` compression exclusion already permits it.
FR6.4 The LLM provider is operator-configured via `OXIDEAN_OXPILOT_*` env
vars — OpenAI, Anthropic, or Ollama adapters (all verified at 1.0.0); the
feature is off unless a provider is configured and the flag is enabled.
FR6.5 The feature flag follows the `OXIDEAN_MCP_ENABLED` +
`admin.mcp.*` env-default + DB-override pattern.
FR6.6 Sessions, messages, tool calls, computer-use targets, and grants
persist in `oxidean-db` with migrations for all three dialects.
FR6.7 Per-user and per-instance message/token budgets are operator-
configurable via `OXIDEAN_OXPILOT_*` env vars with generous defaults, using
the existing per-process sliding-window limiter pattern; the single-replica
limitation is documented (NFR4).

### FR7 — Tool surface (agent ↔ forge)

FR7.1 Repository tools: file read, tree listing, branch info, commit
history (read-only by default).
FR7.2 Change tools: produce a diff for a proposed change; create branch +
PR on approval (FR3.2).
FR7.3 CI tools: list runs/jobs, fetch logs, summarize (FR4).
FR7.4 Connector tools: execute command / read file / write file on a
connected target — always behind consent (FR5.2) and audit (FR5.5).
FR7.5 Issue/PR tools: read issues/PRs and comment only on artifacts the
agent itself created with user approval.

## Non-Functional Requirements

NFR1 Security: session-only auth for `oxpilot.*` (fail-closed PAT
classification); connector auth is PAT-class with hashed verifier material;
explicit consent before any target action; no stored secret ever leaves the
system; secrets-pattern redaction applied to stored transcript content.
NFR2 Privacy: transcripts stored plaintext (disclosed) after redaction;
per-user isolation — a user sees only their own sessions/targets;
connector egress is operator-disclosed in `docs/CONFIGURATION.md`.
NFR3 Performance: first token of a streamed reply should begin within a
reasonable interactive budget on a warm provider (< ~5 s target); the
transcript view keeps incremental composer input responsive (typing echo
under ~100 ms) at a 500-message session as the measurable proxy — marked
aspirational for later hardening; SSE reconnect resumes without
duplicating messages.
NFR4 Reliability: provider failures surface as in-session errors, never
silent stalls; connector disconnects degrade to chat-only gracefully;
single-replica limiter caveat documented.
NFR5 Auditability: connector actions, approvals, and target lifecycle emit
`audit_events`; PR/branch materialization is attributable to the user.
NFR6 Observability: reuse existing logging/metrics surfaces; no new
observability stack.
NFR7 Accessibility: WCAG AA posture per practices — landmarks, heading
order, keyboard operability, `aria-live` for streamed message boundaries,
labelled consent dialogs.
NFR8 Usability: disabled/unconfigured state is explanatory, not a dead
page; errors name the fix (which env var, which doc).
NFR9 Testability: collect-only coverage for `oxidean-api-oxpilot`;
per-crate `procedures_list_matches_dispatch` test; `SessionOnly`
classification pin test; SSE route covered by HTTP-level stack test +
happy-dom client test; connector protocol covered by outbound-HTTP fixture
tests; route + (conditional) browser-coverage manifest rows; new
`oxidean-db` tables ship a `dialect_oxpilot.rs` test that joins the
`db-dialects` `-E` filter and the `docs/TESTING.md` job table in the same
change — otherwise it is dead coverage.

## Constraints

- `ai-octane` is the mandated framework; pin versions, isolate the provider
  surface; OpenHands Agent Server is the recorded fallback.
- Trunk-based: short-lived `feat/` branch → PR → green CI → merge; SemVer
  commit subjects; no AI attribution on commits/PRs.
- New RPC family in a domain crate (mandated); never in `oxidean-api-core`
  residual.
- `apps/web` gate: `make web-lint` + `make web-format-check`; Octane
  `.tsrx` only — no React JSX patterns.
- Rust is the source of RPC types; `make rpc-gen` regenerates the client;
  never hand-edit generated sources.
- DB dialect rules stay inside `crates/oxidean-db`; migrations ×3.
- Every new `OXIDEAN_OXPILOT_*` env var lands in `.env.example` +
  `docs/CONFIGURATION.md`.
- Custody rules (mandated): retrievable secrets AES-256-GCM via env-key
  chain; verifier-class hashed; never return stored secret material from
  agent-reachable surfaces.
- Pairing obligations: route-coverage manifest row per route; SHELLS entry
  per shell; CI job/matrix changes update the `Main` ruleset list in the
  same change.
- Rate limiters are single-replica — acceptable for v1, documented.

## Assumptions

- [assumption] The operator-configured provider (not per-user BYO key) is
  the v1 billing/keys model — confirmed Q1/Q5.
- [assumption] `ai-octane`'s 1.0.0 API is stable enough for this surface;
  a narrow seam keeps churn manageable — carried from feasibility A-5.
- [assumption] Actions read APIs cover the debugger's needs (logs verified;
  run/job listing assumed sufficient — verify in domain design).
- [assumption] Indefinite + user-deletable transcript retention is
  acceptable to operators (no knob in v1) — confirmed Q4.
- [assumption] SessionOnly `oxpilot.*` classification is correct because
  the surface is session-driven; revisit if `ox`/MCP access is wanted —
  confirmed Q4 (practices interview).

## Out of Scope

Autonomous merge/push to protected branches; per-user BYO provider keys;
plugin marketplace; mobile-specific UX; multimodal inputs; agent-to-agent
orchestration; ox-CLI replacement; paid packaging/tiers; GUI/screenshot
computer use; rerun/cancel CI control; operator retention knobs; new
deployable services — the feature rides the existing `api` + `web` tier
(the connector is a user-run client, not a hosted service).

## Open Questions

- Exact `ai-octane` tool/approval API shape in 1.0.0 (approval/interrupt
  surface for consent gates) — resolve in domain-design contract work.
- Whether the context drawer needs live repository diff data or static
  session metadata only — domain design.
- Connector binary distribution channel (downloadable artifact vs
  build-from-source) — infrastructure design.
- Cloud provider defaults vs self-host defaults — deployment design.

## Assumption Confirmation

Confirmed via `requirements-analysis-questions.md` Q1–Q8 (all
conductor-decided under standing instruction 2026-10-27; ratify at PR
review). The five assumptions above carry forward verbatim for ratification
at the domain-design and plan gates.
