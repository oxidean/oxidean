# Contract Design Questions — `/oxpilot`

Inputs: `inception/units-generation/unit-of-work{,-dependency}.md`,
`inception/domain-design/components.md`,
`inception/requirements-analysis/requirements.md`.

## Sources

- [dag] 8 units; edges: U1→all seams, U2→U3/U6/U8, U3→U5/U8, U4→U3
  (soft), U5→U8, U6→U5/U7/U8.
- [req] Public surfaces: `oxpilot.*`/`admin.oxpilot.*` RPC, SSE chat
  routes, 4 connector HTTP routes.

## Q1. Which boundaries get formal contracts here?

- A. Three external contracts — (1) `oxpilot.*`/`admin.oxpilot.*` typed RPC (shared-schema), (2) SSE chat routes + frame vocabulary (yaml spec), (3) connector HTTP routes (OpenAPI) — plus five inter-unit Rust seams as shared-schema blocks: `LlmProvider` (U1→U3/U4), `SessionService` API (U2→U3/U6/U8), `ToolExecutor`/`ToolCard` (U3→U5), action-queue dispatch (U6→U5), `on_session_delete` hook (U2→U3/U6).
- B. External contracts only — Rust traits stay in code review's scope.
- C. Every DAG edge gets a full OpenAPI doc — overkill for in-crate traits.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — 3 external + 5 inter-unit shared-schema seams. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. RPC contract mechanism?

- A. Shared-schema block naming each procedure family, its payload types, and the SessionOnly class — `make rpc-gen` is the source of truth generator; the contract pins families + semantics, not generated signatures.
- B. Full OpenAPI for RPC — wrong; RPC is typed-dispatch, not REST.
- C. Defer to code-generation.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — shared-schema per family. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. SSE + connector versioning policy?

- A. Frame/endpoint versions ride the existing component `versioning.toml` + SemVer discipline; additive frame fields only (consumers ignore unknown), breaking frame changes bump the route path (`/chat/v2/...`) — mirroring how RPC procedures version by name.
- B. Independent version headers per route.
- C. No versioning (internal).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — additive-only within a route; breaking = new path/proc name. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Error/timeout/retry behaviour at boundaries?

- A. RPC: typed `oxpilot.*` error codes (`oxpilot.disabled`, `forbidden`, `budget_exceeded`, …). SSE: terminal frames end streams; reconnect = GET snapshot resume (no Last-Event-ID), exactly-once by message id. Connector: claim long-poll with server-documented backoff hints; report/heartbeat idempotent by action id; revoked/cancelled delivered in poll responses. Contracts pin these, not internals.
- B. Generic HTTP retries everywhere.
- C. Leave to units.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — typed errors, snapshot-resume SSE, idempotent connector posts. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. Contract ownership?

- A. Provider unit owns each spec: U1 owns RPC family list + SSE frame vocab (it ships the skeleton of both); U6 owns the connector protocol doc; trait seams owned by the provider-side unit (U1 `LlmProvider`, U2 `SessionService`, U3 `ToolExecutor`, U6 action-queue + grant DTOs, U2 `on_session_delete`).
- B. A single "contracts" doc owned by nobody's unit.
- C. Consumer units own the contracts they consume.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — provider-side ownership. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- 8 contracts: 3 external (RPC shared-schema, SSE yaml, connector
  OpenAPI) + 5 inter-unit shared-schema seams (Q1, Q5).
- RPC pinned as shared-schema families + semantics (Q2).
- Additive-only evolution; breaking = new path/proc name (Q3).
- Typed RPC errors; SSE snapshot-resume; idempotent connector posts (Q4).

Does this all look correct before I write `contract-summary.md`?

- Looks correct
- Request changes

[Answer]: Looks correct
