# User Stories — Questions

Mode note: this stage ran under the user's standing instruction — "skip human verification until we have a sound implementation on a PR with green CI and reply to questions that might arise yourself through recommendation based on the document I shared." The document is https://www.anthropic.com/engineering/redesigning-source-control (Scion); requirements are the approved `requirements.md` (16 FR groups + 10 NFRs, reviewer-verified). Each answer records the conductor's recommendation and rationale; the user ratifies or overturns any of them at PR review.

## Q1. Who are the personas?

Context: the feature's "users" are mostly machines. The Scion article's audience is agentic coding workloads; Oxidean adds the forge personas that operate it. Requirements FR12/FR13/FR14 name CLI, MCP, and auth surfaces; FR16 puts agent writes in the human-visible activity feed.

- A. Four personas — **Agent** (primary: LLM/agentic tool driving high-frequency writes + reads over RPC/MCP; never holds a full clone), **Developer-Reviewer** (human using `ox vcs` CLI to materialize, inspect, and gate agent work; relies on Git interop for their normal flow), **Forge Operator** (self-host admin: enable flag, quotas, lifecycle, import/export runs, activity visibility), **CI Integrator** (pipelines/tooling calling `vcs.*` via generated client or `ox api`)
- B. Two personas — Agent and Operator only
- C. Six+ personas — split agents by task type (coder/reviewer/tester)
- X. Other (please specify)

[Answer]: A — four personas. The Agent is the design center (the article's entire thesis), but Developer-Reviewer carries the trust story (humans gate consequential machine changes) and Operator carries operability (enable/quota/lifecycle). CI Integrator is thin but real — `ox api vcs.*` and the generated client are its surface. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q2. How are stories broken down?

Context: FRs cluster naturally into capability areas: enablement/lifecycle, commit path (changesets + writes + activity), object store + want-based fetch, derived-data/history queries, refs, working copy, git interop, and surfaces/auth (RPC, CLI, MCP, registration).

- A. By capability cluster aligned to FR groups — 8 story groups (G1 enablement, G2 commit path, G3 objects+fetch, G4 history queries, G5 refs, G6 working copy, G7 git interop, G8 surfaces+auth); each story maps 1+ FRs, IDs `US{group}.{seq}`
- B. By persona — each persona gets its full journey
- C. By implementation unit — mirrors whatever units-generation produces later
- X. Other (please specify)

[Answer]: A — capability clusters. They map cleanly onto the seam structure the codekb already established (crate modules, RPC families, db façade), which makes traceability mechanical and gives units-generation natural boundaries. Persona-journeys still get exercised — every group carries at least one Agent story plus the persona that owns the flow. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q3. What granularity should stories use?

Context: too-coarse stories ("agent commits code") carry no acceptance value; too-fine ones duplicate the FR text. INVEST + Small means each story is one meaningful behavior a consumer can verify.

- A. ~20-26 stories — one per meaningful externally-visible behavior (e.g. "create a changeset without reading a ref", "fetch only the objects I lack", "export a changeset chain to a git branch subject to protection"); each carries 2-5 testable ACs keyed to FR IDs
- B. ~8-10 epic stories — one per FR group
- C. 40+ fine stories — one per FR sub-clause
- X. Other (please specify)

[Answer]: A — one story per externally-visible behavior. That is the granularity contract-design and build-and-test can consume without re-slicing, and it keeps ACs honest (each verifies a wire-level behavior, not a code shape). Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q4. How are stories prioritized?

Context: MoSCoW priorities feed delivery-planning's MVP cut. The requirements' own deferral list (O1–O7) already fixes the Won't line.

- A. Must = the agentic core path (enable, changeset create + idempotent resubmit, object PUT/GET, want-based fetch, ref CAS, basic log/diff/tree/file reads); Should = server-side merge/rebase, blame/path-history, sparse checkout, git import/export, MCP tools, activity feed; Could = operator conveniences (status detail, GC tuning knobs); Won't = O1–O7 (virtualized checkouts, per-dir ACLs, merge queues, tiered daemons, web UI, repo-type conversion, signatures/LFS tiering)
- B. Everything in the FR list is Must — no prioritization
- C. Must = reads only; writes are Should (bootstraps on imported git history)
- X. Other (please specify)

[Answer]: A — the write path is the product (a read-only VCS strand would strand the crate; without cheap writes there is no throughput story). Interop is Should because the protocol must stand alone first — but export is required for the human-trust loop, so it stays in this intent's MVP-adjacent band for delivery-planning to weigh. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q5. What acceptance-criteria style?

Context: ACs must be testable by contract-design and build-and-test without further interpretation; the repo's idiom is typed RPC + real-SQLite tests + protocol smokes.

- A. Behavioral wire-level ACs — each phrased as an observable request/response or state transition ("resubmitting identical changeset content returns the same id; no second row"), each tagged to its FR (e.g. `[FR2.2]`); no implementation-prescriptive criteria
- B. Implementation-level ACs (assert specific internals)
- C. Loose prose ACs
- X. Other (please specify)

[Answer]: A — wire-level behavioral ACs tagged to FR IDs. That is what NFR1's pass/fail bound test and the proc_classes fail-closed gate need downstream, and it keeps the reviewed-behavior boundary clean. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Q6. How do NFRs flow through traceability?

Context: `traceability.json` must enumerate every FR and NFR id. The in-scope `nfr-requirements` stage (3.2) exists specifically to harden NFRs into measurable targets.

- A. FRs → `OK` with story targets; NFRs → `Deferred` targeting `nfr-requirements` (plus `nfr-design` where a design decision must carry them)
- B. NFRs → `OK` claimed against stories anyway
- C. NFRs → `N/A` everywhere
- X. Other (please specify)

[Answer]: A — stories demonstrate FR coverage; NFRs are honestly Deferred to the in-scope `nfr-requirements`/`nfr-design` stages rather than laundered through story text. Conductor-decided under standing instruction (2026-10-18); ratify at PR review.

## Consolidated Summary Confirmation

Summary of confirmed answers:

- **Personas (4)**: Agent (primary), Developer-Reviewer, Forge Operator, CI Integrator.
- **Breakdown**: 8 capability groups mirroring FR clusters — G1 enablement/lifecycle, G2 commit path, G3 objects+fetch, G4 history/derived data, G5 refs, G6 working copy, G7 git interop, G8 surfaces+auth.
- **Granularity**: ~20-26 stories, one per externally-visible behavior, 2-5 behavioral ACs each tagged to FR ids.
- **Priorities**: Must = agentic core write+read path; Should = merge/rebase, blame/path-history, sparse checkout, git import/export, MCP tools, activity feed; Could = operator conveniences; Won't = O1–O7.
- **AC style**: observable request/response or state transitions, FR-tagged, never implementation-prescriptive.
- **Traceability**: FRs → `OK` to story ids; NFRs → `Deferred` to `nfr-requirements`/`nfr-design`.

All six answers are conductor decisions under the standing continuation instruction — each carries rationale and is open to reversal at PR review.

[Answer]: Looks correct
