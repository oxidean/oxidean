# Units Generation — Decomposition Questions

## Q1 — Unit boundary strategy
How should components group into units?

- **A** — By construction-deliverable slice, keeping the domain component boundaries: one unit per cohesive build target (wire/core foundation, DB persistence, each engine component, each host surface). Maps 1:1 to the catalogue so ownership and test seams stay aligned.
- **B** — Coarse feature slices (e.g. "commit path" as one unit spanning core+db+api) — fewer units but each crosses crate boundaries.
- **C** — By crate — one unit per Cargo crate touched (very coarse; one unit would span every component).

[Answer]: A — slice units aligned to the domain components; each unit is a deliverable a construction Bolt can build+verify independently, and the DAG mirrors the component graph so a unit never crosses an ownership boundary. Cross-crate units (core DTOs, db façade, api glue) exist only where the wire contract forces them (foundation unit feeds all).

## Q2 — Unit granularity
Coarse vs fine?

- **A** — 10 units: wire+core foundation, persistence, object store, changeset+ref+index core, merge engine, git interop, API surface (RPC+routes+activity+auth), and three client surfaces (CLI, MCP, web). Each is Bolt-sized (hours–days), not micro.
- **B** — Finer, one per component (~12 units) — more bookkeeping, several units too small to verify alone.
- **C** — Coarser (~4 units: foundation / server engine / surfaces) — units become multi-day and hard to gate.

[Answer]: A — 10 units. MergeEngine and GitInterop are separable because their dependencies are already satisfied by earlier units; lumping them into the core unit would make that unit the size of the whole crate.

## Q3 — Dependency ordering
Strict topological, or parallel where independent?

- **A** — DAG is topology only (this stage's contract); parallelism is flagged but sequencing is Delivery Planning's call.
- **B** — Force a strict serial order here.
- **C** — Fully parallel claim — wrong, persistence+wiring are genuinely ordered.

[Answer]: A — the dependency artifact marks independent sets (CLI/MCP/web surfaces are mutually independent once the API surface exists); 2.9 picks the economic order.

## Q4 — Integration points between units
What couples units?

- **A** — (a) `oxidean-core` DTOs + `proc_classes` + activity constants (foundation → everything); (b) `VcsPersistence` façade signatures (persistence → all stores); (c) `oxidean-vcs` component interfaces (stores/engines → API surface); (d) `ProtectionChecker` trait port + `GitBackend` injection (interop → api host); (e) generated `@oxidean/api-client` + `vcs.*` procedures (api → cli/mcp/web).
- **B** — Units share entities directly (violates ownership).
- **C** — Events/queues — nothing async in v1.

[Answer]: A — contract seams only; no unit reads another's entities.

## Q5 — Deployment model
How do units ship?

- **A** — All embedded in the single existing deployable: `oxidean-vcs` becomes a workspace crate dependency of `oxidean-api` (server feature) and `oxidean-cli` (client feature); no new binary, container, or service. Web unit ships inside `apps/web`.
- **B** — New daemon/sidecar — rejected upstream (in-app mount, ADR-003).
- **C** — Separate package release — no, internal crate.

[Answer]: A — every unit is embedded; "deployment model per unit" is `embedded` across the board (library kind for crate units, ui kind for the web unit).

## Summary confirmation

[Answer]: Looks correct
