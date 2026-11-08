# Architecture Decision Records — `/oxpilot`

Provenance: `domain-design/components.md`; consumes
`inception/requirements-analysis/requirements.md`,
`inception/practices-discovery/team-practices.md`. Format per the
Inception phase guardrails (Context / Decision / Consequences /
Alternatives Rejected).

## ADR-001: Eight-component decomposition

- **Context** — `/oxpilot` spans UI, RPC transport, streaming turn logic,
  third-party providers, an authorization seam (tools), a consent
  lifecycle, and an off-platform client. Options: A) eight components
  (web, api, session service, agent runtime, provider registry, tools,
  gateway, connector); B) five components merging SessionService into
  OxpilotApi and OxpilotTools into AgentRuntime.
- **Decision** — Option A: eight components as catalogued.
- **Consequences** — Positive: SessionService owns all per-user
  invariants (scoping, delete cascade, budgets) in one place; the
  tool-authz seam is explicit and independently testable; the turn loop
  changes at provider-release cadence without touching transport.
  Negative: two more internal call boundaries; more units at
  units-generation.
- **Alternatives Rejected** — Option B: merges a persistence-invariant
  owner into an HTTP layer (design smell per the stage's own rule) and
  buries the security boundary inside the runtime loop.

## ADR-002: New `oxidean-api-oxpilot` domain crate for all `oxpilot.*` procedures

- **Context** — Post-#162 architecture requires new RPC families in their
  own domain crate (machine-enforced by `check-api-domain-graph.mjs`;
  affirmed as a Mandated rule). The residual core crate is forbidden for
  new families.
- **Decision** — `OxpilotApi` is implemented as
  `crates/oxidean-api-oxpilot` with the standard `PROCEDURES` +
  `dispatch` shape, shell registration, `OXPILOT_PROCEDURES` closed-set
  `SessionOnly` classification, `versioning.toml` component, per-crate
  `procedures_list_matches_dispatch` test.
- **Consequences** — Positive: inherits session auth, version gate, and
  fail-closed PAT refusal for free; versioning/changelog plumbing is
  conventional. Negative: boilerplate (crate skeleton, changelog,
  registration arms).
- **Alternatives Rejected** — adding procs to `oxidean-api-core`
  residual: explicitly forbidden by the affirmed rule; it would also
  silently widen the cross-cutting surface.

## ADR-003: Dedicated SSE routes for chat; typed RPC for control plane

- **Context** — Chat needs server→client streaming; typed-RPC dispatch
  does not stream. Precedent `stream_job_log` proves SSE end-to-end on
  `/api/` REST routes (snapshot→wake-append; web excludes
  `text/event-stream` from compression). Interaction-spec pins POST
  (send) via fetch-reader + GET (resume) since `EventSource` is GET-only.
- **Decision** — Two HTTP routes under `/api/oxpilot/sessions/{id}/`:
  `chat` (POST send, streamed response) and `chat/stream` (GET resume),
  mounted in `oxidean-api` `app.rs` with handlers in the oxpilot crate;
  all other calls stay typed RPC.
- **Consequences** — Positive: reuses the proven frame contract and the
  `/api/` edge prefix (zero edge-parity work). Negative: two route kinds
  to document; frame protocol must be specified in contract-design.
- **Alternatives Rejected** — long-poll on `/api/rpc`: no streaming UX,
  more latency per token; a new WS transport: no in-repo precedent.

## ADR-004: Connector rides a runner-style long-poll + report loop

- **Context** — The user's connector must work behind NATs with no
  inbound listener and PAT-class auth (affirmed Mandated rule). Options:
  A) long-poll claim + POST report (Actions runner precedent, Q3-A);
  B) WebSocket; C) SSE+POST.
- **Decision** — Option A, shaped as dedicated HTTP routes (not RPC
  procedures): `POST /api/oxpilot/connector/claim` (long-poll),
  `…/report`, `…/heartbeat`, and `…/pair`. They authenticate
  verifier-class connector credentials — a third credential kind beside
  session cookie and user PAT — checked by ConnectorGateway against
  stored hash material; the SessionOnly classification is untouched
  because these are routes, not procedures. The claim/heartbeat response
  also carries the cancel set, so revoking a grant kills a claimed
  in-flight action at the connector (ADR-004 cancel propagation).
- **Consequences** — Positive: proven transport, ordinary rate limiting,
  audit-friendly; grant checks at claim time keep denied/expired actions
  off the wire; cancel propagation is built into the same poll the
  connector already runs. Negative: claim latency up to the poll window;
  the connector must honor cancel requests promptly (documented client
  contract).
- **Alternatives Rejected** — WebSocket: no infra; SSE: inverts the
  direction and adds a second streaming protocol to a consent-critical
  path.

## ADR-005: Consent/grant state machine owned by ConnectorGateway

- **Context** — Grants span two surfaces (transcript card + target
  lifecycle) and must stay consistent with revoke and target liveness.
- **Decision** — ConnectorGateway owns `Grant` (both connect-scoped,
  `session_id` null, and action-scoped) with states
  pending→allowed|denied|expired|revoked|cancelled; expiry is lazy via
  `expires_at` checked at claim/read; the consent card is a view over
  grant state; AgentRuntime awaits resolution through OxpilotTools.
  Session deletion calls the gateway's revoke/cancel entrypoint rather
  than deleting grant rows directly, so cascade stays audited.
- **Consequences** — Positive: one transition table, one audit point,
  one place that enforces "no dispatch without allowed grant" —
  the exact surface the security model depends on. Negative: the
  session's transcript and the gateway must stay consistent (grant
  result is written back through the tool-call row).
- **Alternatives Rejected** — grants in SessionService: splits consent
  ownership across two components and lets session deletion semantics
  dictate security state; split ownership: two transition tables for one
  lifecycle.

## ADR-006: Rebuildable provider slot + env/DB-override enablement

- **Context** — Providers (OpenAI/Anthropic/Ollama) must be
  operator-configured with an admin override (the `admin.mcp.*`
  pattern), and the feature must fail closed when unconfigured.
- **Decision** — `LlmProviderRegistry` exposes
  `Arc<RwLock<Arc<dyn LlmProvider>>>` on `RpcCtx` (email_slot shape);
  resolution reads `OXIDEAN_OXPILOT_*` env then `oxpilot_settings`
  override; `None` → `oxpilot.disabled` denials; provider keys are
  AES-256-GCM retrievable-class secrets under the OXPILOT key chain.
- **Consequences** — Positive: live provider swap without restart;
  matches the affirmed flag pattern; custody rule satisfied.
  Negative: rebuild plumbing + settings table.
- **Alternatives Rejected** — `Option<Shared*>` boot-only (vcs shape):
  a config change would need a restart, unlike the MCP flag operators
  already know; per-request construction: re-validates and re-reads
  config on every turn.

## ADR-007: `ox pilot connect` ships the connector inside the `ox` CLI

- **Context** — The client needs secure distribution and PAT-class
  credentials on user machines.
- **Decision** — `OxpilotConnector` is an `ox` subcommand family
  (`ox pilot …`), reusing the CLI's binary matrix, auth storage, and
  `Out::Rpc` conventions.
- **Consequences** — Positive: zero new distribution; users already
  trust `ox`; pairing UX = one command. Negative: `ox` gains a
  long-running mode; needs the server-feature flag honored client-side.
- **Alternatives Rejected** — standalone binary: new distribution +
  signing surface; shell script: no credential custody, no fs sandboxing.

## ADR-008: Agent write tools are proposal-scoped and identity-bound

- **Context** — The agent must propose changes and comment on issues/PRs
  it created (FR7.5) without gaining general write power or any
  autonomous merge/push path.
- **Decision** — OxpilotTools routes reads through `rpc_dispatch` as the
  session user; the only writes are: ProposedDiff artifacts,
  branch+PR creation on explicit user confirmation, and comments on
  agent-created artifacts. Every write tool call is a ToolCall row +
  audit event.
- **Consequences** — Positive: the "agent never merges" guarantee is a
  tool-surface property, not a prompt instruction; review surface is
  small. Negative: limits some later automations (accepted for v1).
- **Alternatives Rejected** — full repo write tools: unnecessary for v1
  and widens the blast radius; prompt-level "don't merge" rules: not
  enforceable.

## ADR-009: Persistent UsageCounter rows carry budgets; the limiter stays per-process

- **Context** — FR6.7 mandates per-user and per-instance budgets, and the
  team-practice rate-limit surface is per-process sliding-window limiters
  on credential endpoints. The first draft left budget state unowned and
  refusal claimed by two components.
- **Decision** — `SessionService` owns `UsageCounter` rows (per-user and
  per-instance windows, `user_id` nullable for the instance row) and is
  the single owner of the over-budget refusal decision; OxpilotApi
  delegates the check. Credential-surface rate limiting remains the
  separate per-process `FailedAuthLimiter`-class mechanism on OxpilotApi.
- **Consequences** — Positive: budgets survive restarts, are observable,
  and have exactly one owner; the two "limiter" notions are explicitly
  separated. Negative: an extra table + write per accepted turn.
- **Alternatives Rejected** — per-process in-memory budget counters:
  resets on restart and wrong on multi-replica deployments; refusal in
  OxpilotApi with counters elsewhere: two owners for one decision.

## ADR-010: Turn lifecycle — provider pinned per turn, delete aborts before cascade

- **Context** — Two races were unowned: a provider-slot rebuild mid-turn
  could swap providers half-stream, and session delete during an active
  turn could orphan the stream or race the cascade.
- **Decision** — AgentRuntime pins the `Arc<dyn LlmProvider>` snapshot at
  turn start; OxpilotApi aborts any live turn (terminal frame +
  cancellation token) before invoking the SessionService delete path,
  which in turn calls ConnectorGateway's audited revoke/cancel
  entrypoint for the session's grants/actions.
- **Consequences** — Positive: deterministic provider identity per reply;
  delete is a coordinated stop, not a row wipe. Negative: needs a
  per-session cancellation handle in the runtime registry.
- **Alternatives Rejected** — re-reading the provider per turn segment:
  mid-reply provider switches corrupt the tool-call protocol; deleting
  rows under a live stream: orphan frames + unaudited grant orphans.

## Assumptions & Open Questions

- [assumption] The SSE frame protocol (snapshot/append/terminal/tool
  frames) is specified in contract-design; this stage fixes only the
  transport shape (ADR-003).
- [assumption] The `repo.actions.*` read surface names used by
  OxpilotTools (runs/jobs/log-read procedures) are verified in
  contract-design against the dispatch registry; the design only fixes
  that reads route through rpc_dispatch, never direct DB/git.
