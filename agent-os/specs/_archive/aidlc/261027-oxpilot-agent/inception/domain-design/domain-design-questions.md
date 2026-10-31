# Domain Design Questions — `/oxpilot`

Upstream inputs: `inception/requirements-analysis/requirements.md` (FR1–FR7,
NFR1–NFR9), `inception/user-stories/stories.md` (US1.1–US5.5),
`codekb/oxidean-app/architecture.md` (Intent Seam Map),
`codekb/oxidean-app/component-inventory.md`,
`inception/practices-discovery/team-practices.md`.

## Sources

- [desc] Intent: `/oxpilot` agentic chat/builder/debugger with opt-in computer use.
- [scope] Workflow-selected scope: `feature`.

## Q1. Component decomposition?

The feature needs: UI, control-plane RPC, turn loop (agent runtime),
provider abstraction, tool surface, connector gateway (server side),
connector client (user side), session persistence.

- A. Eight components: `OxpilotWeb`, `OxpilotApi` (domain crate), `SessionService` (sessions/transcripts/tool-calls/budgets/delete cascade), `AgentRuntime` (turn loop: prompt→provider stream→tool calls→diffs), `LlmProviderRegistry` (trait + OpenAI/Anthropic/Ollama + enablement), `OxpilotTools` (tool registry + per-tool authz), `ConnectorGateway` (pairing/targets/grants/action dispatch), `OxpilotConnector` (user-run client). Persistence stays in `oxidean-db` as an external dependency.
- B. Coarser: fold SessionService into OxpilotApi, tools into AgentRuntime — five components.
- C. Finer: split transcript persistence from session lifecycle.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — eight components; each owns a distinct lifecycle and entity set. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. Provider slot shape on `RpcCtx`?

- A. `Option<SharedLlm>` (`vcs`-style, validated at boot; `oxpilot.disabled` wire error when absent) — simplest, but an admin-settings toggle would need a restart to reconfigure providers.
- B. Rebuildable `Arc<RwLock<Arc<dyn LlmProvider>>>` (`email_slot`-style) — admin `admin.oxpilot.*` DB-override settings can swap provider/key live, matching the affirmed feature-flag pattern.
- C. Per-request construction — no shared slot at all.
- D. Not decided.
- X. Other (please specify)

[Answer]: B — rebuildable slot; the affirmed `admin.mcp.*` env+DB-override flag pattern requires live reconfiguration. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Connector transport (server side of the dial-out channel)?

- A. Long-poll `fetch_task`-style loop + POST result reports — mirrors the proven Actions runner protocol (claim → execute → report); no new transport machinery; grants/timeouts ride ordinary rows.
- B. WebSocket bidirectional channel — lowest latency, no in-repo WS precedent.
- C. Server→connector SSE + POST results — inverts the natural dial direction; SSE precedents are all server→browser.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — long-poll + POST reports, mirroring the Actions runner loop (proven PAT-class transport). Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Connector client packaging?

- A. `ox pilot connect …` subcommand on the existing `ox` CLI — reuses distribution (cross-platform binary matrix), PAT-class auth, and the `ox api` escape hatch.
- B. Standalone `oxidean-connector` binary — separate distribution/channel.
- C. Shell script only.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `ox` subcommand; distribution and credential handling already exist. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Grant/consent state machine owner?

- A. `ConnectorGateway` owns Target/Grant/Pairing entities and the pending→allowed|denied|expired|revoked transitions; the UI's consent card is a read+answer view; the agent runtime waits on the gateway for grant resolution.
- B. `SessionService` owns grants (they live "in" a session).
- C. Split: session grants in SessionService, target pairing in ConnectorGateway.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — ConnectorGateway owns the whole consent lifecycle; one owner, one transition table. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Decomposition: 7 components — web, api crate, session service, agent runtime, provider registry, tools, connector gateway + connector client (Q1).
- Provider slot: rebuildable `Arc<RwLock<Arc<dyn LlmProvider>>>` on RpcCtx (Q2).
- Connector transport: long-poll + POST reports, Actions-runner-style (Q3).
- Connector client: `ox pilot connect` subcommand (Q4).
- Consent state machine: owned wholly by ConnectorGateway (Q5).

Does this all look correct before I generate the component catalogue and ADRs?

- Looks correct
- Request changes

[Answer]: Looks correct
