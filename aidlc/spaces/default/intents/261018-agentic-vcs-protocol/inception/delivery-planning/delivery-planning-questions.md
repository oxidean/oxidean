# Delivery Planning — Questions

Strategic questions for the Bolt sequence (a Bolt = one build pass over a piece of the work, ending in something that runs). Answered under the standing instruction.

## Strategic

**Q1 — Sequencing strategy.** Risk-first, value-first, thin end-to-end slice first, or a mix?

[Answer]: Mix — dependency-order with the storage spine first. The DAG forces U1→U2→U3→U4 before anything else; within that constraint the first Bolt (U1+U2) is the lowest-value but unblockable spine, and U3+U4 give the first runnable proof (a changeset round-trip inside the crate). Value lands at U7 (first wire-visible surface) and U8/U9 (the agent-facing surface Anthropic's design optimizes for). Risk is concentrated in U5 (merge engine) and U6 (git interop + protection path) — they ride second so their failure modes surface before client surfaces are built on them.

**Q2 — Formal scoring (WSJF-style)?** If so, weights on risk / value / size?

[Answer]: No formal WSJF — the DAG leaves little reorderable freedom (a serial chain U1→U4, one parallel pair U5/U6, then U7, then the three consumer surfaces). Where choice exists the tie-break is risk-reduction first (merge + interop before clients), then value. Recorded as rationale rather than a scoring model.

**Q3 — Bolt size.** One Unit per Bolt, several related Units, or cross-cutting slices?

[Answer]: Several related Units per Bolt, grouped by DAG layer: B1 {U1+U2}, B2 {U3+U4}, B3 {U5+U6}, B4 {U7}, B5 {U8+U9+U10}. Each Bolt ends in something that runs: crate-level round-trip (B2), wire-visible API (B4), full agent surface (B5).

**Q4 — Parallel Bolts?**

[Answer]: No — Construction Execution stays `serial` (the recorded default). Inside B3/B5 the units are parallel-capable (U5∥U6, U8∥U9∥U10) but unit-major serial ordering is preserved for a single-author run; grouping records the parallelism option without exercising it.

**Q5 — External dependencies?**

[Answer]: None outside this repo. Every seam is internal: `oxidean-db` façade, `oxidean-git` backend trait, `ProtectionChecker` port, `proc_classes`/`PROCEDURES`/`system.manifest` registration, `@oxidean/api-client` regeneration, Octane settings surface. The only env-shaped input is the `OXIDEAN_VCS_*` operator surface — all five defaults frozen in contract-design.

**Q6 — Biggest build worry to tackle early?**

[Answer]: The GC-vs-upload race (object PUT → commit window) and the merge engine's conflict fidelity are the two correctness cliffs — both land in B2/B3, ahead of the wire surface. Second-order worry: `insert_repo_activity` signature generalization (nullable push columns) touches a shared façade — it's small but crosses unit boundaries, so it rides B1/B2 (U2 owns the façade change).

## Per-Bolt (collected during artifact generation)

Recorded inline in `bolt-plan.md` — units bundled, skeleton marker (n/a — skeleton off affirmed), Definition of Done, confidence hypothesis, owning mob (all Bolts: aidlc-developer-agent, since team-formation was SKIP under classic scope).
