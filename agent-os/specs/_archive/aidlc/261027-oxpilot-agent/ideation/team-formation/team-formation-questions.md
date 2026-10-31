# Team Formation Questions — `/oxpilot` Agentic Chat / Builder / Debugger

Upstream inputs: `scope-definition/scope-document.md`,
`scope-definition/intent-backlog.md`, `feasibility/feasibility-assessment.md`
(this stage consumes all three).

## Sources

- [desc] Initial description: "Fully self-driven I want you to plan and build a full setup of a agentic chat/builder/debuger that has its own /oxpilot page, has computerUse (that allows the end-user to connect the agent to their local system or a remote system to their own liking and configuration). For the framework I want to use https://github.com/TanStack/ai/tree/main/packages/ai-octane and please go for best effort around standards, UI design/UX, security/privacy and usability. I want you to self-drive this until we have a new branch with a PR against main with green CI. Once CI is green I want you to trigger coderabbit AI for review, poll for its response, fix them and do this until coderabbit doesnt find anything anymore. To trigger a review, check the comment it posts or just comment `@coderabbitai review` yourself"
- [scope] Workflow-selected scope: `feature`.

## Q1. Who is available for this initiative?

Context: delivery is a single conductor-driven workflow under the maintainer's standing self-drive delegation; human review happens at the PR.

- A. One conductor adopting the stage-graph's agent personas inline (product, architect, developer, quality, design, ops, compliance) + the maintainer as PR reviewer/ratifier + CodeRabbit as automated reviewer. No additional humans allocated; consistent with the standing instruction.
- B. A dedicated multi-person team is assigned.
- C. External contractors/AWS Pro Services are needed.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — one conductor adopting stage-graph personas inline + maintainer as PR ratifier + CodeRabbit as automated reviewer; no additional humans allocated per the standing instruction. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q2. What capacity does that team represent?

Context: no dedicated headcount; the conductor is the sole execution lane and CI/coverage gates are the throughput ceiling.

- A. Single execution lane, quality-gated: throughput is bounded by CI cycle time and gate cadence, not headcount. No parallel human capacity; construction runs serial (unit-major iteration per the intent config). Parallel human work on the same branch is excluded by construction policy.
- B. Multiple parallel builders available.
- C. Capacity unknown.
- D. Not assessed.
- X. Other (please specify)

[Answer]: A — single execution lane, quality-gated: throughput bounded by CI cycle and gate cadence; serial unit-major construction; no parallel human capacity on the same branch. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q3. Which skills are required vs. available?

Context: backlog P-1..P-9 spans Octane UI, Rust RPC/domain work, dialect migrations, protocol/security design, e2e/browser testing, and operator docs.

- A. Required = Octane/.tsrx UI authoring, TanStack ai-octane integration, Rust API + RPC procedure work, oxidean-db ×3 migrations, streaming/SSE through the edge, connector protocol + security design, browser/e2e test authoring, docs/operator config writing. Available = all of the above via the persona set the workflow already carries (developer, architect, devsecops, quality, design, delivery, ops) plus in-repo skills (octane, rust-async-patterns, tdd). No external skill gap identified.
- B. Significant skill gaps exist requiring hiring/training.
- C. Skill profile unassessed.
- D. Not needed.
- X. Other (please specify)

[Answer]: A — required skills (Octane/.tsrx, ai-octane integration, Rust RPC/domain, ×3 dialect migrations, SSE/edge streaming, connector security design, browser/e2e tests, operator docs) are all covered by the stage-graph persona set + in-repo skills; no external gap. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q4. Are there competing initiatives?

Context: the repo runs parallel worktrees, but this workflow is serialized and owns its own intent branch/worktree.

- A. No competing initiative inside this intent — serialized, single-line delivery; other repo worktrees are outside this intent's scope and can't block the stage gates (only shared CI time).
- B. A parallel initiative shares the same delivery lane.
- C. Unassessed.
- D. Not applicable.
- X. Other (please specify)

[Answer]: A — no competing initiative inside this intent; serialized single-line delivery; other worktrees share only CI time. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Q5. What is the mob/team composition per construction unit?

Context: construction runs unit-major serial with checkpoints; the stage graph assigns lead personas per stage; supports join where stages declare them.

- A. Per-unit composition follows the stage graph: product (requirements/user-stories), architect (domain/contract/functional/NFR/infrastructure design), developer (code-generation + build-and-test), devsecops/compliance folded into NFR stages, design (mockups), delivery (planning), ops (operation stages); quality evidence lands per unit per the testing posture. No separate mob roster — persona assignment is stage-driven.
- B. A fixed mob owns all units end-to-end.
- C. A swarm of parallel agents per unit.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — stage-graph persona assignment per unit (product/architect/developer/design/delivery/ops/devsecops as stages declare); quality evidence lands per unit; no separate mob roster. Conductor-decided under standing instruction (2026-10-27); ratify at PR review.

## Consolidated Summary Confirmation

- Team: one conductor adopting stage-graph personas + maintainer ratifying at PR + CodeRabbit automated review (Q1).
- Capacity: single serial execution lane; CI cycle + gate cadence bound throughput (Q2).
- Skills: all required skills covered by the persona set + in-repo skills; no external gap (Q3).
- Competing initiatives: none inside this intent (Q4).
- Composition: stage-graph persona assignment per unit; per-unit quality evidence (Q5).

Does this all look correct before I generate the artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct


## Assumption Confirmation

The artifacts carry these assumptions:

- [assumption] The maintainer's PR review is assumed to be the only required human checkpoint for the whole delivery — no additional staffing or sign-off path is needed beyond the standing instruction.
- [assumption] The stage-graph persona set plus in-repo skills is assumed sufficient for every proto-unit; if a unit surfaces a genuine specialist need (e.g. dedicated security review of the connector protocol), it is raised at that unit's stage rather than resourced now.

- A. Accept assumptions
- B. Convert to follow-up questions

[Answer]: A. Accept assumptions — conductor-decided under standing instruction (2026-10-27); ratify at PR review.
