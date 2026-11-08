# Units of Work — `/oxpilot`

Provenance: consumes `inception/domain-design/components.md`,
`inception/domain-design/decisions.md`,
`inception/requirements-analysis/requirements.md`,
`inception/user-stories/stories.md`; plan approved in
`units-generation-questions.md`. Units are component-aligned (Q1); the DAG
in `unit-of-work-dependency.md` is topology only — sequencing is
Delivery Planning's call.

## Unit table

| Unit ID | Name | Directory | Kind | Deploy | Complexity |
|---|---|---|---|---|---|
| U1 | skeleton-slice | `u1-skeleton-slice` | service | embedded (api + web) | M |
| U2 | session-core | `u2-session-core` | service | embedded (api) | M |
| U3 | agent-runtime | `u3-agent-runtime` | service | embedded (api) | L |
| U4 | providers | `u4-providers` | service | embedded (api) | M |
| U5 | tools | `u5-oxpilot-tools` | service | embedded (api) | L |
| U6 | connector-gateway | `u6-connector-gateway` | service | embedded (api) | L |
| U7 | connector-client | `u7-connector-client` | service | embedded (ox cli) | M |
| U8 | oxpilot-ui | `u8-oxpilot-ui` | ui | embedded (apps/web) | L |

All units are `embedded` — they ship inside the existing `oxidean-api`,
`apps/web`, and `ox` deployables; no new service (requirements Out of
Scope).

## U1 — skeleton-slice (the walking skeleton)

Owns: the thin end-to-end chat round-trip — new `oxidean-api-oxpilot`
crate skeleton (PROCEDURES + dispatch + registration: workspace member,
shell dep, `extend_from_slice` + fan-out arm, `versioning.toml`
component, per-crate CHANGELOG + `procedures_list_matches_dispatch`
test); `OXPILOT_PROCEDURES` pinned `SessionOnly` set + closed-set test;
`oxpilot.status/session_create/session_list` minimal procs; oxpilot
session/message tables (migrations ×3 + `dialect_oxpilot.rs` in the
`db-dialects` `-E` filter + TESTING.md row); the two SSE routes
(`/api/oxpilot/sessions/{id}/chat` POST + `…/chat/stream` GET) mounted in
`app.rs`; a stub provider behind the enablement gate (`oxpilot.disabled`
when flag/provider absent); minimal `/oxpilot` page + `$session`
workspace shell (SHELLS + astro + `.tsrx` + RESERVED_TOP_SEGMENTS +
routeTree + coverage-manifest rows); `OXIDEAN_OXPILOT_*` env block in
`.env.example` + `docs/CONFIGURATION.md`.

Boundary: delivers only the round-trip — no real provider, no tools, no
targets/grants, no consent cards. U1 owns the `LlmProvider` trait + the
`RpcCtx` slot + one internal stub implementation behind the env gate;
U4 later plugs the real registry/adapters into that slot (U3's soft
provider edge therefore resolves to U1's declared seam, not to U4).
Verifies route/auth/crate/SSE seams before breadth lands (skeleton-on
requirement).

## U2 — session-core

Owns `SessionService`: full session/message/tool_call/queued/proposeddiff/
usage_counter persistence, per-user scoping enforcement, delete cascade,
secrets-redaction on write, budget counters + over-budget refusal,
transcript history reads for prompts. The delete cascade ships as its
own rows plus a revocation-hook seam (`on_session_delete` entrypoint);
U6 registers the gateway revoke/cancel handler and U3 registers the
abort-live-turns handler when those units land — U2's own cascade is
complete and verified alone, and each later registration extends it
(phased integration, not an undeclared dependency).
depends_on: U1 (tables + crate exist).

## U3 — agent-runtime

Owns `AgentRuntime`: turn loop, prompt assembly (repo context +
transcript window), provider streaming → SSE frame emission (snapshot/
append/tool/terminal contract), per-turn provider pinning, queue drain
on turn end, abort-on-delete coordination, one-active-turn-per-session
enforcement. Depends on U1 + U2. Consumes U4's registry through the
`RpcCtx` slot — a stub satisfies it until U4 lands (boundary documented
in the DAG file).

## U4 — providers

Owns `LlmProviderRegistry`: OpenAI/Anthropic/Ollama adapters over the
`LlmProvider` trait defined in U1 (outbound `reqwest`, wiremock-fixture
tests — never live providers), the registry implementation that
rebuilds the U1-installed `RpcCtx` slot, `oxpilot_settings` override
table + `admin.oxpilot.*` procs, provider-key AES-256-GCM custody under
the `OXIDEAN_OXPILOT_*` key chain. depends_on: U1. Parallel-eligible
with U2/U3.

## U5 — oxpilot-tools

Owns `OxpilotTools`: the tool catalog the model can call — repo read
tools (file/tree/branch/log/issues/PRs — names verified against the
dispatch registry in contract-design), actions run/job/log reads
(read-only), ProposedDiff creation + patch view, branch+PR
materialization on explicit user confirm (provenance line in PR body),
agent-created artifact comments only (FR7.5). Per-tool authorization +
write-tool audit emission. Also owns the computer-use exec/fs tool
surface the agent calls for US4.3: tool definitions, argument
validation, and dispatch into `ConnectorGateway`'s action queue — the
tools never reach a target directly; they enqueue `Action` rows the
paired connector claims. depends_on: U3 (tool-call protocol) and U6
(action dispatch + grant checks).

## U6 — connector-gateway

Owns `ConnectorGateway`: pairing (device-grant-style single-use codes,
shown once), target registry + liveness + per-user scoping, Grant state
machine (pending/allowed/denied/expired/revoked/cancelled; connect +
action grants; lazy expiry), Action lifecycle (queued→claimed→running→
done/failed/cancel_requested→cancelled) + cancel set in claim/heartbeat
responses, connector HTTP routes (`pair`, `claim`, `report`,
`heartbeat`) with gateway-verified verifier-class credentials, consent
lifecycle audit. depends_on: U2.

## U7 — connector-client

Owns `OxpilotConnector`: `ox pilot connect`/`pair`/`disconnect`
subcommands in the `ox` crate — pairing flow, credential custody
user-side, claim→execute→report loop with documented backoff, local
exec/fs sandbox honoring the granted capability set, cancel handling,
no secret echoing into results. depends_on: U6.

## U8 — oxpilot-ui

Owns `OxpilotWeb` beyond the U1 shell: full index (sessions + targets
sections, empty states), complete workspace (rail+transcript+drawer),
all cards (tool-call, diff, consent, queued), dialogs (new session,
connect/pair two-step, confirms), SSE client (POST fetch-reader + GET
resume, snapshot-replace+append), flag-off state, WCAG AA items,
route/browser coverage rows for every interactive surface.
depends_on: U3 (real stream contract), U5 (diff card), U6 (consent
surfaces). The U1 page skeleton counts as its foundation.

## Assumptions & Open Questions

- [assumption] U3 may start before U4 completes by coding against the
  `LlmProvider` trait + a stub adapter (the registry slot shape is fixed
  in contract-design); marked as a soft edge, not a hard dependency.
