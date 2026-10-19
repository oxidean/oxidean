# Risk & Sequencing Rationale — Agentic VCS Protocol

The ordering heuristic is **dependency-order with risk-reduction tie-breaks** — not formal WSJF (Reinertsen). Rationale for declining a scoring model: the Unit DAG (`unit-of-work-dependency.md`) removes nearly all sequencing freedom — a forced serial chain U1→U2→U3→U4, one parallel pair (U5∥U6), then U7, then the three consumer surfaces U8/U9/U10. Where choice exists, the tie-break is risk-reduction first, value second, size last.

## Why this order

- **B1 {U1,U2} first — the unblockable spine.** Wire types and the persistence façade are the two layers everything else calls; U2 also carries the only shared-code change (`insert_repo_activity` nullable push columns, C4), so landing it first means every later unit builds on the final signature. Low standalone value but nothing else can run — classic dependency-first placement.
- **B2 {U3,U4} second — the first runnable proof.** Object store + changeset core deliver an in-crate round-trip (commit → manifest → log → diff → blame) before any wire surface exists. The GC-vs-upload race (grace window) and create-tx index update are the first real correctness cliffs — surfaced here, not in B4's assembly.
- **B3 {U5,U6} third — risk before clients.** Merge conflict fidelity and the Git interop path (id-convergent import, lazy import, protection-checked export through `ProtectionChecker` + hook-driven ref updates) are the two hardest correctness surfaces. They precede the wire surface so their failure modes surface while the blast radius is still crate-internal.
- **B4 {U7} fourth — first wire-visible proof.** The API surface is pure assembly once the engines exist: dispatch, flag gate (two-case), routes, activity rows. Putting it after B3 means the first external check exercises the *whole* stack — commit→fetch→merge→import→export — rather than a shell.
- **B5 {U8,U9,U10} last — the agent-facing payoff.** CLI, MCP, and the web toggle consume frozen contracts; building them last means every client is written against stable wire shapes. The throughput claim (agent commit/log/diff with no checkout) is demonstrated end-to-end here.

## Deviations from strict topological order

None — B1→B2→B3→B4→B5 is a valid path through the DAG with zero reordering. The only liberty taken is bundling DAG-layer siblings into single Bolts (U5∥U6, U8∥U9∥U10 share a gate); that is a gate-granularity choice, not an ordering one.

## Risks carried forward (not resolved by sequencing)

- **Merge fidelity under real git shapes** — diff3-text granularity may under-serve binary/semantic conflicts; contract pins `{path,kind,detail?}` so the failure surface is typed, but fidelity is a B3 test burden.
- **Shared-façade blast radius** — `insert_repo_activity` signature change touches existing push paths; mitigated by landing it in B1 with the existing suite as the regression net.
- **GC correctness under concurrent agents** — grace window + link-join is the chosen mechanism; its concurrent-create test is a B2 DoD item, not a deferred check.
