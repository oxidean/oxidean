# Risk & Sequencing Rationale — `/oxpilot`

Sequencing argument: **skeleton-first, then dependency-respecting value
order** — no numeric WSJF (Weighted Shortest Job First; a value÷size
scoring model) because a hard DAG with eight units and one mandated
skeleton leaves little for scoring to decide. The ordering below is
topological with a one-line value/risk note each; parallel-eligible
sets exist but execution is serial (one build lane).

## Order and why

1. **skeleton-slice** — scope-mandated walking skeleton: proves
   crate+RPC+SSE+route end-to-end before breadth. Highest risk-reduction
   per unit size: it falsifies the whole integration shape early.
2. **session-core** — everything downstream reads/writes through it;
   persistence+budget ownership is independent of the runtime (per the
   C8 hook seam), so landing it early isolates data-model mistakes.
3. **providers** — parallel-eligible with U2; placed here because U3 is
   next and a real registry beats the stub for exercising the turn loop.
   (Could equally run second — serial lane just needs a pick.)
4. **agent-runtime** — the turn state machine is the product's riskiest
   logic (streaming, reconnect, races); lands mid-plan with all seams
   real.
5. **connector-gateway** — consent/cancel correctness is the feature's
   security core; before tools/UI so U6's surfaces exist when consumers
   build.
6. **oxpilot-tools** — needs U3's tool seam + U6's action queue (DAG
   edge U5→U6); builder/debugger value rides a stable runtime.
7. **connector-client** — leaf unit behind U6; late because it is
   testable entirely against gateway fixtures.
8. **oxpilot-ui** — DAG fan-in: consent cards, diff cards, target
   manager all need U5/U6 surfaces. UI-last is structural, not a value
   judgment — the U1 skeleton page already gave early visual feedback.

## DAG-conformance check

Bolt order U1,U2,U4,U3,U6,U5,U7,U8 is a valid topological ordering of
`unit-of-work-dependency.md`: U2,U4 after U1; U3 after U1+U2; U6 after
U2; U5 after U3+U6; U7 after U6; U8 last. No deviation to justify.

## Risks ranked (tackled early where possible)

- SSE/reconnect semantics — mitigated by skeleton (B1) + runtime (B4).
- Consent/cancel correctness — B5 lands before consumers (B6, B8).
- Provider API drift — B3 fixtures pin request shapes; adapters isolate.
- Scope size — one-Bolt-per-unit keeps each checkpoint demo-able; any
  Bolt slipping is visible at its own gate.
