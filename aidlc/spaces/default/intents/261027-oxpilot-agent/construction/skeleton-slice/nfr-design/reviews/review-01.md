## Review

**Verdict:** READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1

### New findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | minor | security-design.md — Check ordering design (Q4) | The drawn interface cannot serve the gate-exempt caller it names. `fn enablement(ctx) -> Result<EnablementState, AppError>` used as `enablement()?` on gated procs/handlers implies disabled ⇒ `Err(oxpilot.disabled)`; but `oxpilot.status` "calls enablement() to REPORT {enabled, provider, reason}" — it cannot obtain `{enabled:false, reason}` from an `Err`, and reporting disabled state is that proc's whole purpose (NFR1.6, BR5.4). The alternate reading (Ok carrying a disabled `EnablementState`) leaves the gated `?` with no deny step shown. | Pin the two-level shape: a resolver returning `EnablementState` (status reports it; it never denies) plus a guard step mapping `!enabled` → `oxpilot.disabled` (gated surfaces `?`-propagate). | New |
| R-02 | minor | reliability-design.md — Failure-mode design; logical-components.md — Turn task blast radius | The panic path is claimed ("panic/error terminates that turn only") but has no mechanism. The table covers `Result` errors, transport close, and process death; nothing covers a panic in the spawned task. On unwind, either the ActiveTurn-entry removal (end-of-task code) is skipped → the session is pinned at `409 turn_active` until restart, or the entry drops while the assistant row stays `status=streaming` → a phantom-live transcript the startup-only orphan sweep never reaps mid-process (the NFR4.3 invariant, in-process variant). | Pin panic finalization: a drop-guard / supervised `JoinHandle` that both removes the entry and lands the row's terminal status (`error`) on `JoinError`. | New |
| R-03 | minor | performance-design.md — Stream-path design, Mid-turn attach row | `emitted_deltas` is given a fixed `capacity 256`, but NFR3.7 bounds the ring "to the in-flight message's delta count" and entities.md states no capacity. A fixed cap changes the guarantee: a message emitting >256 deltas evicts early buffered appends, and a mid-turn attach then depends on an unstated invariant — that the persisted row's content (the snapshot base) covers every delta ≤ the `last_delta_seq` watermark, i.e. row-content flush lag < ring capacity. With lazy row flushes, deltas between watermark+1 and the ring's oldest retained entry are undeliverable → exactly-once (NFR4.4) breaks for that attach. | Either bound the ring per-message or pin the flush/watermark invariant that makes 256 safe. | New |
| R-04 | minor | performance-design.md — Index & read-path design, Status/create row | The cell contradicts itself: "One read + one insert respectively" pairs status→read, create→insert, then the same cell says `oxpilot.status` is "pure in-process state … zero DB reads". Separately, enablement resolution reads the `oxpilot_settings` override seam per WF1 — if U1's seam issues a probe read (or relies on absent-table ⇒ env fallback), status is not strictly zero-read, and the probe cadence is unstated. | Recast the cell: create = repo-resolve read + session insert; status = in-process env+slot resolution with the settings seam's read behaviour named. | New |
| R-05 | minor | traceability.json — NFR7.1 + NFR8.1 coverage rows | Coverage targets cite content that lives only in upstream artifacts, not in any nfr-design document. NFR7.1's "landmark/headings/labels" decision appears in tech-stack-decisions.md (a requirements-stage artifact); logical-components.md's web-bundle row (the other cited target) names only page/queries/SSE-client. NFR8.1's "explanatory flag-off state naming env vars + docs" is pinned by BR5.4 upstream; reliability-design.md's NFR4.5 row says only "the page renders the explanatory state". | Either land the baseline-semantics and flag-off-copy decisions in a design artifact or mark these rows as upstream-inherited rather than designed. | New |
| R-06 | minor | performance-design.md — Stream-path design, Keep-alive row | "`Sse::keep_alive` interval ~30 s" hedges a pinned contract value: C2 and NFR3.3 require a ping ≥ every 30 s of stream silence, so a "~" interval that can drift above 30 s violates the pin it cites. | Pin interval ≤ 30 s (e.g. exactly 30 s). | New |
| R-07 | minor | reliability-design.md — Failure-mode design, NFR4.3 row | "Startup orphan sweep" does not pin sweep-vs-accept ordering. If the sweep runs concurrently with request-accept, `status='streaming'` rows with no owning task are readable by snapshots in the window — the phantom-live state the sweep exists to prevent. | Pin "sweep completes before the process serves traffic" (or state how the window is closed). | New |
| R-08 | minor | reliability-design.md — Health & test hooks, NFR4.10 | The failure-injection flag is offered as "(env/constructor flag)" without noting the consequence of the env option: a new `OXIDEAN_OXPILOT_*` variable falls under BR1.5, which requires `.env.example` + `docs/CONFIGURATION.md` entries in the same change. | Pick the mechanism (constructor-only avoids the env surface) or acknowledge the docs-parity obligation. | New |
| R-09 | minor | logical-components.md — Inventory, `redact()` row | A false negative is labelled "(accepted — disclosed)", but the NFR2.1 disclosure covers storage practice (plaintext transcript + redact-on-write + per-user isolation), not detector recall. "Disclosed" overclaims unless the CONFIGURATION.md text explicitly states the pattern-table detects only known secret shapes. | Reword to "accepted residual risk" or pin the disclosure copy to name the limit. | New |

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
None.

### Validation notes

- Repo spot-checks: `crates/oxidean-core/src/proc_classes.rs` confirms fail-closed `SessionOnly` for unlisted procs ("never callable with a token") — SessionOnly-by-omission grounded, and the "`ox` bearer refused" claim is accurate (`ox` authenticates via device-grant/PAT tokens, both classified). `crates/oxidean-runner/src/exec/mod.rs` `mask_secrets(text, mask_values)` confirmed as known-value masking — cannot serve NFR1.8; the new `redact()` detector is correctly required.
- Consistency checks pass: detach-to-complete, orphan sweep, Lagged→snapshot resync, per-process ActiveTurn, single-replica posture, 256-frame broadcast capacity, 64 KiB ⇒ `422 invalid_body`, and reason precedence `flag_off > misconfigured > no_provider` are stated identically across the five design docs and match nfr-requirements + functional-design (post-R-18/R-19: `active_turn{turn_id, started_seq, message_id, last_delta_seq}`, message_id pre-minted at claim).
- Traceability: all 53 upstream NFR ids (NFR1.1–1.13, NFR2.1, NFR3.1–3.18, NFR4.1–4.10, NFR5.1–5.3, NFR6.1–6.5, NFR7.1, NFR8.1–8.2) are present in `upstream_ids` with coverage rows; no missing ids.
- Format: designs are pattern/decision tables; the only code block is a 5-line interface sketch (≤15-line constraint met).

### Summary

No blocking defects: every requirement has a concrete mechanism or an explicitly recorded deferral, and the cross-doc invariants hold. The nine minors are precision gaps, not contradictions — the strongest are the EnablementGuard signature that cannot serve `oxpilot.status` as drawn (R-01), the unpinned panic finalization that undercuts the stated blast radius (R-02), and the fixed-256 `emitted_deltas` cap that quietly weakens the exactly-once replay guarantee for long messages (R-03). All are one-to-three-line clarifications best landed before infrastructure/code design consumes the seam.

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |
