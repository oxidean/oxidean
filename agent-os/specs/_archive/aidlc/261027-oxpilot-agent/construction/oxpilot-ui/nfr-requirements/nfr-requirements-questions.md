# NFR Requirements Questions — oxpilot-ui

Unit: `oxpilot-ui` (U8, kind `ui`) — `OxpilotWeb` beyond the U1
shell: the full M1 index (sessions + Computer use sections,
empty/loading/error states, delete confirm), the M3 new-session
`Dialog`, the M4 connect/pair two-step `Dialog`, the M2 three-pane
workspace (sessions rail, boundary-announced transcript live region +
composer, context drawer), all in-transcript cards (`ToolCallCard`,
`DiffCard`, `ConsentCard`, `QueuedRow`), the queued-send path, the M5
confirms, the M7 flag-off on both routes, the SSE client extensions
(`tool_call`/`card` frame kinds, snapshot-replace + append), and the
coverage-manifest rows.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U8.
- [req] `inception/requirements-analysis/requirements.md` — NFR1
  (session-only auth binds the procs; U8's half is the render-side
  injection boundary + consent-surface honesty + shown-once pairing
  custody), NFR2 (per-user isolation renders; secrets never on
  screen), NFR3 (SSE reconnect without duplication; typing-echo
  < ~100 ms at 500 messages as the measurable proxy — aspirational),
  NFR4 (failures surface in-session, never silent stalls), NFR5
  (mutations attributable via real C1 procs), NFR6 (reuse existing
  surfaces — no new observability stack), NFR7 (WCAG AA — U8 is THE
  owning unit), NFR8 (explanatory disabled state; errors name the
  fix), NFR9 (route + browser coverage rows; happy-dom client test).
- [design] `construction/oxpilot-ui/functional-design/
  {functional-spec,rules,frontend-components}.md` — WF1–WF12,
  BR1–BR11 the NFRs bind to; READY on adversarial review.
- [contract] `inception/contract-design/contract-summary.md` — C1
  `oxpilot.*` proc families + error codes + the flagged amendments;
  C2 SSE frame vocabulary + non-2xx error set + the 409
  `turn_active` forbidden pair ("auto-enqueue and silent accept are
  both forbidden") + the card-ref resolution table.
- [ix] `inception/refined-mockups/interaction-spec.md`,
  [mock] `mockups.md` M1–M8,
  [a11y] `accessibility-checklist.md` (WCAG 2.1 AA pins),
  [ds-map] `design-system-mapping.md` (primitive map +
  `components/oxpilot/` + state-ownership table).
- [domain] `inception/domain-design/components.md` §OxpilotWeb —
  "Local UI state only… the UI enforces nothing itself."
- [u3-fd] `construction/agent-runtime/functional-design/` — the
  live stream contract (`tool_call` frames + `call_id`/`call_index`,
  `card{queued}`, chained turns via `active_turn`).
- [u5-fd] `construction/oxpilot-tools/functional-design/` —
  `card{diff}`/`card{consent}` emit points; `diff_apply` user-only.
- [u6-fd] `construction/connector-gateway/functional-design/` —
  pairing→pending-grant flow; `lifecycle`/`pairing_id`; read-time
  re-mint.
- [shipped] `apps/web/src/routes/oxpilot.tsrx`,
  `oxpilot.$session.tsrx`, `lib/oxpilot-chat.tsrx`,
  `lib/oxpilot-queries.ts`, `lib/oxpilot-stream.ts` +
  `oxpilot-stream.unit.test.ts`, `oxpilot.integration.test.ts`,
  `oxpilot.browser.test.tsx`, both coverage manifests,
  `packages/api-client` (every C1 proc generated),
  `components/settings/copy-command.tsrx`, `components/ui/*`
  primitives, `lib/{toast,markdown,parse-unified-diff,
  highlight-diff,lazy-diff-rows,relative-time}.ts`.
- [codekb] `codekb/oxidean-app/technology-stack.md` — Octane
  `.tsrx` (not React), `@octanejs/tanstack-query`, unified/remark/
  rehype (GFM + sanitize), Vitest + happy-dom + Playwright.

## Q1. Which NFR domains does oxpilot-ui carry real obligations in?

- A. Security (the render-side injection boundary — every server-,
  model-, and target-supplied string renders as text until the
  pinned GFM-on-complete path; shown-once pairing-code custody;
  consent-surface honesty — the UI enforces nothing itself per
  [domain], so it must also fabricate nothing), Privacy (per-user
  isolation renders — foreign session `not_found` ⇒ fatal close +
  back link; secrets and pairing codes never reach transcript,
  live region, logs, or clipboard beyond the pinned strings),
  Performance (frame-path render cost, dedupe, boundary-only
  announcements, the NFR3 typing-echo proxy at 500 messages),
  Reliability (EventSource reconnect→snapshot-reseed correctness,
  at-least-once dedupe, frozen-card truthfulness, dead-mount
  writes, no silent dead surfaces), Accessibility (NFR7 — U8 is THE
  owning unit; every [a11y] pin binds here), Usability (NFR8 —
  explanatory flag-off, errors naming the fix/env/doc),
  Observability (zero UI-side telemetry; toasts + typed banners as
  the user-facing record), Testability (NFR9 — manifest rows +
  stream unit tests are this unit's surface-closure leg).
- B. Accessibility + usability only — misses that the SSE client,
  the injection boundary, and the consent/pairing surfaces live
  only here; no other unit can carry NFR3's typing-echo proxy or
  NFR4's reconnect correctness for this transport.
- C. Defer security to the server units — the server's typed errors
  and ownership checks are real, but the DOM is where assistant
  text, `args_summary`, target labels, and diff patches either stay
  text or become markup; that boundary exists only in this unit.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Security (the render-side injection boundary — every server-,. Conductor-decided under standing delegation; ratify at PR review.

## Q2. Stream-frame render budget — how does the transcript stay inside the typing-echo proxy?

- A. Append-only rendering at frame cadence: `append` frames dedupe
  `(message_id, seq)` then extend ONLY the matching `liveTail`
  entry — never a whole-transcript re-render; assistant text is
  plaintext `whitespace-pre-wrap` while streaming (no GFM re-parse
  per delta — the FD Q8 pin); `aria-live` announcements fire at
  message START/END boundaries only, never per frame; autoscroll
  writes run only on appended rows while scrolled to bottom
  (BR5.6). NFR3's measurable proxy stands: composer typing echo
  < ~100 ms at a 500-message session — aspirational for later
  hardening, recorded here as the design bar.
- B. Re-render the transcript per frame behind a virtualized list —
  no windowing lib ships for transcripts (`lazy-diff-rows` is
  diff-scoped); inventing one is unpinned code and a live-region
  hazard.
- C. Batch frames on a flush timer (~100 ms) — delays first-token
  paint past the cadence the UX pins and adds a flush path that
  must still reseed correctly on snapshot.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Append-only rendering at frame cadence: `append` frames dedupe. Conductor-decided under standing delegation; ratify at PR review.

## Q3. Bundle and lazy-load posture — what may the unit add to the web build?

- A. Zero new dependencies: `components/oxpilot/` imports only
  shipped primitives (`Dialog`/`AlertDialog`/`Checkbox`/
  `RadioGroup`/`Select`/`EmptyState`/`Button`/`Input`/`Label`/
  `Textarea`/`List`/`Badge`/`toaster`) and shipped libs
  (`renderGfm`, `parse-unified-diff`/`highlight-diff`/
  `lazy-diff-rows`, `relative-time`, `toast`, `CopyCommand` reuse).
  GFM parse runs only on completed assistant text; the diff pane
  rides the shipped `lazy-diff-rows` seam so large patches never
  block the transcript path; route-level splitting stays as
  shipped — the two `/oxpilot*` routes.
- B. Add an incremental-GFM streaming-parser dep — a new dependency
  for the per-delta re-parse behavior already rejected at FD Q8-C;
  the sanitize posture of an unproven parser is also a new review
  surface.
- C. Lazy-mount every card kind behind per-kind dynamic imports —
  suspense boundaries inside `aria-live` regions fragment
  announcements; the card set is four components sharing one
  dispatch.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Zero new dependencies: `components/oxpilot/` imports only. Conductor-decided under standing delegation; ratify at PR review.

## Q4. Accessibility conformance bar — pinned or advisory?

- A. WCAG 2.1 AA per [a11y] is the conformance bar and every pin is
  acceptance-checked, not advisory — BR10 verbatim: exactly one
  `h1` per route, `nav`/`main`/`complementary`/`form` landmarks,
  sequential headings, `aria-live="polite"` + `aria-atomic="false"`
  at message boundaries only, labelled focus-trapped
  `Dialog`/`AlertDialog` with Esc + focus restore (confirms focus
  Cancel), `role="group"` consent card with Deny always reachable,
  real labelled `Checkbox`es, status never color-only,
  focus-within (not hover-only) row actions, ≥4.5:1/≥3:1 contrast,
  the pinned `Ctrl/⌘+Enter` keydown handler backing the
  `aria-describedby` hint, and the autoscroll/jump-pill scroll
  contract.
- B. Advisory a11y pass post-ship — NFR7 names AA posture and the
  acceptance criteria (AC1.2.3, AC4.2.x) gate on the same pins;
  advisory status ships a known non-conformance.
- C. Widen to full WCAG 2.2 AA now — the [a11y] checklist is
  2.1-scoped; expanding the bar mid-unit adds unpinned criteria.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — WCAG 2.1 AA per [a11y] is the conformance bar and every pin is. Conductor-decided under standing delegation; ratify at PR review.

## Q5. SSE resilience budget — how much reconnect smarts does the client own?

- A. The shipped split stays (FD Q4-A): `POST …/chat` streams over
  the fetch-reader (a request body rules out `EventSource`);
  `GET …/chat/stream` attaches over `EventSource` with native
  auto-reconnect ON — every reconnect replays a `snapshot`, which
  reseeds client state wholesale (no `Last-Event-ID` exists — C2;
  exactly-once is `(message_id, seq)` dedupe). `es.onerror` runs
  exactly one `session_get` probe: `not_found`/`oxpilot.disabled`
  ⇒ fatal `close()` + typed banner; any other result ⇒ native
  reconnect continues to the next snapshot. Never blind infinite
  retry, never a dead spinner (BR9.3). Unmount/session-switch
  closes the stream and cancels readers — nothing writes into a
  dead mount.
- B. Custom backoff/jitter loop over the attach — re-implements
  native reconnect badly; the contract's reseed model already makes
  reconnect cheap and correct.
- C. Fail hard on the first transport drop — transient blips would
  evict a healthy session; the one-shot probe exists precisely to
  distinguish fatal from transient.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — The shipped split stays (FD Q4-A): `POST …/chat` streams over. Conductor-decided under standing delegation; ratify at PR review.

## Q6. Pairing code + secrets on screen — the display rules?

- A. The minted `pairing_code` renders ONCE in step 2's read-only
  `CopyCommand` field showing `ox pilot pair <code>` verbatim —
  never re-fetched, never written to browser storage, never
  rendered into the transcript live region, never logged, and
  dropped with the dialog (AC4.1.4, BR7.3). No other secret-class
  value renders anywhere: C1 payloads carry none (provider keys are
  `key_set`/`last4` on the admin surface, which this unit does not
  render); consent cards show `args_summary` — the server's
  bounded, redacted copy — never raw command text.
- B. Stash the code in `sessionStorage` for step-2 retry — survives
  the dialog's own lifetime and invents a client-side secret copy
  the contract never asks for.
- C. Echo the code into the transcript for context — the transcript
  persists server-side; that IS the violation AC4.1.4 names.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — The minted `pairing_code` renders ONCE in step 2's read-only. Conductor-decided under standing delegation; ratify at PR review.

## Q7. Coverage/test strategy — how does the UI prove the new surfaces?

- A. Extend the shipped trio, no new test infra (BR11, FD Q12-A):
  `route-coverage.manifest.ts` keeps its two `oxpilot.*` happy-dom
  rows on `oxpilot.integration.test.ts` — extended to the targets
  section, dialogs, card kinds, flag-off, and the queue path;
  `browser-coverage.manifest.ts` rebinds the repo-`Select` row
  onto `new-session-dialog.tsrx` and gains subject-keyed rows
  only on gate-discovered surfaces — `oxpilot-new-session`,
  `oxpilot-connect` (the gate orphan-fails browser rows on
  undiscovered files; button-only surfaces ride the happy-dom
  route rows — narrowed at infra-design review);
  `oxpilot-stream.unit.test.ts` covers the new `tool_call`/`card`
  frame variants + append dedupe.
  Stack-browser e2e stays out of scope (spec Scope).
- B. Add Playwright stack-browser e2e suites for the workspace —
  the spec scopes e2e out explicitly; happy-dom plus the existing
  browser project cover the pinned rows without a new suite.
- C. Component-only unit tests — the manifest rows are the
  registered coverage contract (NFR9); isolated component tests
  leave route gating and the SSE seam unproven.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Extend the shipped trio, no new test infra (BR11, FD Q12-A):. Conductor-decided under standing delegation; ratify at PR review.

## Q8. Telemetry + error-copy stance — what does the UI report, and how do errors read?

- A. No UI-side telemetry of any kind (spec Observability pin —
  server audit at U5/U6 owns who/what/when): no analytics, no
  beacons, no console metrics, no error-reporting dep. The UI's
  observability contribution is the honest surface: every mutation
  rides a real C1 proc (never a fabricated state), success toasts
  via `toast.ts`, and every negative path renders a typed textual
  notice that names the fix — `budget_exceeded` names the
  `OXIDEAN_OXPILOT_*` env vars + CONFIGURATION.md (AC5.2.2),
  `turn_active` explains the queued path, flag-off names the
  flag/provider/doc. Copy tone stays plain sentence-case per the
  M8 strings ("The provider stopped mid-reply — your messages are
  saved.").
- B. Add a lightweight client error beacon — a new egress channel
  on a surface the spec pins silent, and an untrusted-content leak
  path besides.
- C. Toast on failures too — toasts are transient and
  non-focusable; typed failures need the inline/banner surfaces
  that persist until the user acts (BR9.1).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — No UI-side telemetry of any kind (spec Observability pin —. Conductor-decided under standing delegation; ratify at PR review.

## Consolidated Summary Confirmation

- Domains: Security (render/custody posture), Privacy, Performance,
  Reliability, Accessibility (owning unit), Usability,
  Observability, Testability — eight with real pins; Scalability
  recorded as bounded-per-session surface area (Q1).
- Perf: append-only frame path, plaintext-while-streaming,
  boundary-only announcements, bottom-only autoscroll; typing-echo
  < ~100 ms @ 500 messages kept as the aspirational proxy (Q2).
- Bundle: zero new deps; shipped primitives/libs only; lazy diff
  rows; route splitting unchanged (Q3).
- A11y: WCAG 2.1 AA pinned — BR10 items are acceptance-checked
  (Q4).
- SSE: EventSource native reconnect → snapshot reseed; one-shot
  `session_get` probe splits fatal from transient; full cleanup on
  unmount (Q5).
- Secrets: pairing code shown once in `CopyCommand`, never
  transcript/storage/log; `args_summary` only on cards (Q6).
- Tests: shipped manifests extended — happy-dom route rows,
  subject-keyed browser rows, stream unit tests; no e2e suite (Q7).
- Telemetry/errors: zero UI telemetry; success toasts; typed
  fix-naming notices on every negative path (Q8).

[Answer]: Looks correct
