# Code Generation Questions — oxpilot-ui (U8)

Unit: `oxpilot-ui` — full `/oxpilot` index + three-pane workspace over the
U1/U2/U3/U5/U6/U7 seams (14 new `components/oxpilot/*.tsrx` files, lib
stream/query extensions, route reworks, manifest rows, extended test
trio), plus the three flagged C1 amendments the functional design assigns
this unit (`session_create.target_id` + `computer_use_target_id` reads +
`0090`, `grant_list{session_id}`, `queue_delete`).

The code-generation plan (`code-generation-plan.md`) drafts the full
build order + structural pins. The questions below settle the open
implementation decisions before code lands — recommended options are
marked.

## Q1. Implementation batching order

The plan sequences Group 1 (server amendments + `rpc-gen`) → Group 2
(lib) → Group 3 (components) → Group 4 (routes) → Group 5 (manifests) →
Group 6 (tests). Is that the batching you want?

- A. Keep the plan's order — server amendments + `rpc-gen` first so the
  generated `target_id`/`computer_use_target_id`/`session_id`/
  `queueDelete` surface exists before any UI code consumes it, then lib,
  then components bottom-up, then routes, manifests, and tests.
  (recommended — every downstream layer calls only landed surface)
- B. UI-first — land the lib + component + route layers against the
  current generated client, then add the server amendments last.
- C. One batch — everything lands in a single change with no internal
  ordering.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Keep the plan's order — server amendments + `rpc-gen` first. Conductor-decided under standing delegation; ratify at PR review.

## Q2. Do the flagged server amendments land in this unit?

The functional design carries three flagged additive C1 amendments and
assigns their ownership to THIS unit: `0090_oxpilot_session_target.sql`
×3 + `session_create.target_id` + `computer_use_target_id` on the
`session_get`/`session_list` reads; `grant_list{session_id}`; and
`queue_delete`. Ship them here or defer?

- A. Land all three in this unit. (recommended — the design names U8
  their owner; the target picker, rail/drawer binding, and the 4-read
  mount/reseed set all consume them; the refused-amendment degrades are
  already recorded in the spec)
- B. Land `session_create.target_id`/`0090` + `grant_list{session_id}`
  but defer `queue_delete` (the QueuedRow keeps the "still queued —
  send after retry" line only).
- C. Defer all three — UI-only unit; the reseed runs the degraded 3-read
  set and the target radio/binding drop entirely.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Land all three in this unit. Conductor-decided under standing delegation; ratify at PR review.

## Q3. Test-file extension strategy

The shipped seam trio exists and is bound by the coverage manifests:
`lib/oxpilot-stream.unit.test.ts`, `routes/oxpilot.integration.test.ts`
(route-coverage rows), `routes/oxpilot.browser.test.tsx`
(`oxpilot-repo-trigger` browser row). Where do the new assertions go?

- A. Extend the shipped trio in place — the manifest bindings keep
  pointing at the same test files; the stream unit test gains the
  frame-union/`readyState` assertions, the happy-dom file gains every
  surface + grep-pin assertion, the browser file gains the two new
  subject rows. (recommended — BR11/Q12-A binds each row to that exact
  test file; a new file is bound by no manifest row and would
  orphan-fail the gate)
- B. New per-component test files (e.g. `components/oxpilot/*.unit.test.ts`
  or a second integration file) alongside the shipped trio.
- C. Mixed — extend the shipped trio AND add a components-scoped
  integration file for the card/dialog assertions.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Extend the shipped trio in place. Conductor-decided under standing delegation; ratify at PR review.

## Q4. Does `queue_delete` get a UI control?

The spec flags `queue_delete` as the third C1 amendment and pins the
M8/[ix] inline cancel on `QueuedRow` as its consumer — "v1 ships
without the affordance only if the amendment is refused." Assuming Q2
lands the proc, does the control ship too?

- A. Proc surface + its design hook — `queue_delete` lands AND the
  `QueuedRow` inline cancel wires `queueDelete{session_id, queued_id}`
  then invalidates `queue_list` (the same proc-and-control pattern the
  Manage expander rides for `grant_respond`). (recommended — proc-only
  surface with no consumer is dead wire, which the honest-surface
  posture rejects)
- B. Proc-only — `queue_delete` ships server-side; the queued row keeps
  the "still queued" line with no cancel affordance this pass.
- C. No proc at all — drop amendment #3; ship "still queued — send
  after retry" only (the recorded refusal fallback).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Proc surface + its design hook — `queue_delete` lands. Conductor-decided under standing delegation; ratify at PR review.

## Q5. How is the `grant_list{session_id}` refusal fallback coded?

The spec records the degraded path: a refused `grant_list{session_id}`
amendment narrows the mount/reseed set to `diff_list{session_id}` only;
pending consent then surfaces through the Manage-expander's
`grant_list{target_id}` path (BR7.4) — the recovery claim narrows,
never silently. How does that degradation live in the code?

- A. Code-shape degradation — `reseed()` runs the 4-read set
  (`fetchOxpilotSnapshot` + `sessionGet` + `grantList{session_id}` +
  `diffList{session_id}`) only because the amendment lands here (Q2-A);
  if the amendment were refused, the generated
  `OxpilotGrantListRequest.session_id` field wouldn't exist, the
  grant-side reseed arms + card reconstruction are deleted rather than
  branched, and the degraded 3-read set IS the shipped code — decided
  at this gate, never probed at runtime. (recommended — matches the
  spec's recorded refusal path; no dead conditional)
- B. Runtime feature-detect — `reseed()` calls
  `grantList{session_id}` inside a try/catch and silently narrows on a
  proc error.
- C. Build/env flag — a compile-time toggle selects the 3- vs 4-read
  set.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Code-shape degradation — `reseed()` runs the 4-read set. Conductor-decided under standing delegation; ratify at PR review.

## Consolidated Summary Confirmation

Decisions recap:
- Batching: server amendments + `rpc-gen` first, then lib → components
  → routes → manifests → tests (Q1-A).
- Amendments: all three flagged C1 deltas land in this unit (Q2-A).
- Tests: extend the shipped manifest-bound trio; no new test files
  (Q3-A).
- `queue_delete`: proc + the `QueuedRow` inline-cancel hook, per design
  (Q4-A).
- `grant_list{session_id}` refusal: code-shape degradation — the 4-read
  set exists only with the amendment; no runtime probing (Q5-A).

Does this all look correct before I write the code-generation artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct

## Plan Approval

[Approval Fingerprint]: sha256:v3:8cd89d3492d2638e3dcdfe9ea8b4da0fae21e13c7ca308afcde720a28ea93b73
[Planned Source]: 477594ea2649fac3041ee8efae3f6410ba955f710bde00f74b8dc32b6a7328b0

- "Approve Plan": proceed to code generation
- "Request Changes": revise the plan

[Answer]: Approve Plan
