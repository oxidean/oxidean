# Unit Dependency DAG — `/oxpilot`

Topology only — no recommended build order or critical path here; those
are Delivery Planning (2.9) decisions. Edges read "A depends on B".

## Machine-readable edges

```yaml
units:
  - name: skeleton-slice
    kind: service
    depends_on: []
  - name: session-core
    kind: service
    depends_on: [skeleton-slice]
  - name: agent-runtime
    kind: service
    depends_on: [skeleton-slice, session-core]
  - name: providers
    kind: service
    depends_on: [skeleton-slice]
  - name: oxpilot-tools
    kind: service
    depends_on: [agent-runtime, connector-gateway]
  - name: connector-gateway
    kind: service
    depends_on: [session-core]
  - name: connector-client
    kind: service
    depends_on: [connector-gateway]
  - name: oxpilot-ui
    kind: ui
    depends_on: [skeleton-slice, session-core, agent-runtime, oxpilot-tools, connector-gateway]
```

## Prose DAG

```
skeleton-slice (U1)
 ├─▶ session-core (U2) ─▶ agent-runtime (U3) ─▶ oxpilot-tools (U5) ─┐
 ├─▶ agent-runtime ─────────┘                     │                │
 ├─▶ providers (U4) ══soft═▶ (U3 via U1 trait)    │                │
 │           connector-gateway (U6) ◀─────────────┘                │
 │                  │                    │                          ▶ oxpilot-ui (U8)
 │                  └─▶ connector-client (U7)     │                │
 └─▶ session-core ──▶ oxpilot-ui (U8 reads session state) ◀────────┘
```

## Walking skeleton

U1 (`skeleton-slice`) is the first DAG unit and the integrated slice:
it runs end-to-end before any other unit exists — `/oxpilot` page →
`oxpilot.session_create` → stub provider → SSE stream back to the page,
behind the `oxpilot.disabled` gate. Every later unit extends a running
system; nothing else is needed for U1 to run.

## Integration points between units

- U1 → all: crate skeleton, wire DTOs, `OXPILOT_PROCEDURES` set, SSE
  route shape, flag gate — the seams every unit plugs into.
- U2 → U3/U6/U8: `SessionService` read/write/budget/queue entrypoints;
  U6 consumes U2's delete-cascade hook; U8 reads session/queue state via RPC.
- U4 → U3: the `LlmProvider` trait + slot lookup (`ctx.oxpilot_provider()
  -> Option<Arc<dyn>>`) — soft edge; U3 builds against trait + stub.
- U3 → U5/U8: `ToolExecutor`/`ToolCard` trait + SSE frame vocabulary
  (tool/terminal kinds) for U5; the SSE client contract for U8.
- U6 → U5: the action-queue dispatch + grant-check entrypoint the
  computer-use exec/fs tools call (OxpilotTools→ConnectorGateway, the
  edge declared in `components.md`).
- U6 → U7/U8: the four connector HTTP routes + Action/Grant DTOs for
  U7; target-status + consent-card RPC surface for U8.
- U5 → U8: ProposedDiff DTOs + the `oxpilot.diff_apply` proposal-PR proc.

## Parallel development opportunities

- After U1: {U2, U4} independent.
- After U2: {U3, U6} independent.
- After U3+U6: {U5, U7} independent.
- U8 is the fan-in — it waits on U1+U2+U3+U5+U6. Its non-consent
  surfaces could start earlier against U3 alone if Delivery Planning
  splits the unit (recorded, not prescribed).

## Cycle check

No unit appears in its own transitive `depends_on`; the graph is acyclic.
