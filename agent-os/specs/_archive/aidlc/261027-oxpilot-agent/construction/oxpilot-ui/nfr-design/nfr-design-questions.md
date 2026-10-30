# NFR Design Questions — oxpilot-ui

Unit: `oxpilot-ui` (U8, kind `ui`) — `OxpilotWeb` beyond the U1
shell: the full M1 index (sessions + Computer use sections, M5
confirms, M7 flag-off), the M3 new-session `Dialog`, the M4
connect/pair two-step `Dialog`, the M2 three-pane workspace
(sessions rail, boundary-announced transcript live region +
composer, context drawer), all in-transcript cards
(`ToolCallCard`, `DiffCard`, `ConsentCard`, `QueuedRow`), and the
SSE client extensions (`tool_call`/`card` frame kinds,
snapshot-reseed, the `es.onerror` probe).

## Sources

- [nfr-req] construction/oxpilot-ui/nfr-requirements/*.md —
  NFR1.x–NFR8.x/NFR-Sx/TS-x rows the designs must satisfy (READY).
- [design] functional-design/{functional-spec,rules,
  frontend-components}.md — WF1–WF12 + BR1–BR11 + the component
  inventory the designs land on (READY).
- [contract] contract-summary.md — C1 `oxpilot.*` procs + the
  flagged amendments, C2 frame vocabulary + non-2xx error set +
  the 409 `turn_active` forbidden pair + the card-ref table.
- [code] apps/web/src/routes/{oxpilot,oxpilot.$session}.tsrx,
  lib/{oxpilot-stream,oxpilot-queries,oxpilot-chat,
  markdown,highlight-diff,parse-unified-diff,lazy-diff-rows,
  relative-time,toast}.*, components/repo/markdown-body.tsrx
  (`MarkdownHtml` — the shipped `dangerouslySetInnerHTML` seam),
  components/settings/copy-command.tsrx, components/ui/*
  primitives — the shipped seams U8 extends.

## Q1. Assistant-text render boundary — where does the plaintext-vs-GFM split live in code?

- A. Two-phase render inside `OxpilotTranscript`/`OxpilotMessageRow`:
  `status === "streaming"` rows render `whitespace-pre-wrap` text
  nodes ONLY (Octane `{expr}` interpolation writes text, never
  markup) + the `▌` cursor + the "Oxpilot is replying…" hint —
  zero parse cost and zero markup interpretation per delta
  (NFR3.2). On `complete` the row's FINAL text passes
  `renderGfm()` ONCE → `MarkdownHtml` — the shipped
  `dangerouslySetInnerHTML` seam in
  `components/repo/markdown-body.tsrx` whose input is the
  `rehypeSanitize`-carrying toolchain's output (lib/markdown.ts).
  The unit's innerHTML-class sinks are a TWO-entry allowlist:
  `MarkdownHtml` (the path above) plus the shipped
  `HighlightedDiffLine.contentHtml` highlight sink that
  `diff-card`/`context-drawer` feed from `escapeHtml`-built
  content. A grep pin on `components/oxpilot/**` + both oxpilot
  routes asserts no `dangerouslySetInnerHTML` outside those two
  and no `innerHTML`/`outerHTML`/`insertAdjacentHTML`/
  `document.write` anywhere (rides the route-coverage test). Every non-assistant string — tool name,
  `args_summary`, `output_summary`, patch lines, target `label`,
  session `title`, grant `capability`/`args_summary`, queued
  `content` — stays a text node in `<code>`/`<pre>` wrappers by
  construction and never approaches the seam (NFR1.2–1.5).
- B. Per-delta incremental GFM — rejected at FD Q8-C: re-parse
  cost per frame + partial-fence flicker + a hostile string
  interpreted mid-stream; the plaintext-while-streaming pin
  exists to make the hostile interval unparsed.
- C. `MarkdownBody` (the async render-on-prop-change wrapper) for
  assistant rows — same `renderGfm` pipeline but mounted per row
  with an effect that re-runs on every text change: on a
  streaming row it re-parses per delta — the pinned split forbids
  it; `MarkdownBody` suits static bodies (blob/issue text), not
  the transcript lifecycle.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Two-phase render inside `OxpilotTranscript`/`OxpilotMessageRow`:. Conductor-decided under standing delegation; ratify at PR review.

## Q2. `es.onerror` posture — how does the probe split fatal from transient, and what renders?

- A. `openOxpilotStream(sessionId, onFrame, { onFatal })`:
  `es.onerror` checks `es.readyState` first; on CLOSED it fires
  exactly ONE `apiClient.oxpilot.
  sessionGet{session_id}` probe per failure episode — a real wire call
  through the generated client (the same `{code, message}` decode
  `lib/oxpilot-queries.ts` uses), NOT a `fetchQuery` cache read
  (a cached hit inside the 10 s `staleTime` would mask a
  just-deleted/foreign session). The discriminator is
  `es.readyState`: CONNECTING means a network drop with native
  retry already in flight (no probe spent — next `snapshot`
  reseeds wholesale); CLOSED means an HTTP-denial class (401/403/
  429/5xx permanently fail the EventSource) — `es.close()` + ONE
  `session_get` names it (`not_found` ⇒ notice + back link;
  `oxpilot.disabled` ⇒ flag-off swap; any other code ⇒ generic
  stream-error banner via `onFatal(code)` — NFR1.7/1.8/NFR4.3). The
  probe is one-shot per CLOSED episode and its own throw degrades
  to the generic banner — never a loop, never a dead
  spinner.
- B. Probe through TanStack `fetchQuery` on the sessionGet
  options — cache-aware reads make the fatal check
  `staleTime`-blind; the probe exists precisely because the wire
  answer (deleted vs. down) is what the cache cannot know.
- C. Retry-budget counter (close after N drops) — a counter still
  cannot distinguish a foreign session (permanent) from a network
  blip (transient); the probe answers what the counter guesses.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `openOxpilotStream(sessionId, onFrame, { onFatal })`:. Conductor-decided under standing delegation; ratify at PR review.

## Q3. Mount/reseed card-recovery — what shape is the seed, and how does the refused-amendment degrade?

- A. `reseed()` runs the pinned 4 reads as parallel awaits folded
  into ONE `Seed{version, messages, streamingIds, activeTurn,
  pendingQueue, toolCalls, cards, session}`:
  `fetchOxpilotSnapshot` + `sessionGet` + `grantList{session_id}`
  + `diffList{session_id}`; `setSeed(version+1)` remounts
  `OxpilotChatSurface` keyed `s-${version}` so persisted rows and
  reconstructed cards paint in ONE mount (NFR3.6's bound). Card
  reconstruction mapping: pending grant ⇒ pending
  `OxpilotConsentCard`; non-pending grant ⇒ frozen outcome card;
  pending proposal ⇒ pending `OxpilotDiffCard`; applied/absent ⇒
  post-state card. Reconstructed cards append AFTER the persisted
  transcript rows — they name LIVE affordances (BR5.5's
  still-actionable guarantee), not transcript position; the
  durable rows' order is the wire's. If the flagged C1
  `grant_list{session_id}` amendment is refused, the read set
  drops to 3 (snapshot + `session_get` + `diff_list`) and pending
  consent surfaces through the Manage expander's
  `grant_list{target_id}` path only — the narrower claim recorded
  per spec Negative paths, never silently (NFR4.9).
- B. Replay-only recovery (trust the attach's `tool_call`/`card`
  frame replay) — card frames die with their turn's
  `emitted_deltas` buffer; a remount after terminal shows no
  pending affordances — the exact defect BR5.5/WF6 exists to
  close.
- C. Per-card lazy fetch on render — N card frames ⇒ N proc reads
  at paint time and a second paint wave after the keyed remount;
  the 4-read mount set is the pinned bound and the remount is the
  atomic boundary.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `reseed()` runs the pinned 4 reads as parallel awaits folded. Conductor-decided under standing delegation; ratify at PR review.

## Q4. a11y announce + focus machinery — where do the boundary announcements and focus contracts live?

- A. Two-channel announce on `OxpilotTranscript`: the container
  keeps `aria-live="polite" aria-atomic="false"` verbatim
  (BR10.1), and rows enter/change inside it ONLY at boundaries —
  the streaming delta tail + `▌` cursor render inside an
  `aria-hidden="true"` subtree so per-frame text mutations never
  queue announcements (a polite region announces every DOM
  mutation; stream cadence would chatter — BR10.1's exact failure
  mode); the "Oxpilot is replying…" hint is normal in-region text
  whose appearance IS the START announcement; on `complete` the
  row swaps its full text in-region — the END announcement. A
  visually-hidden `role="status"` announcer sibling carries the
  explicit boundary strings a text-swap can't voice ("Action
  request — allow or deny" on consent-card appearance, once —
  NFR7.2). Focus machinery: route-open effect calls
  `textareaRef.focus()` on the composer; rail/drawer Esc inside a
  pane returns `toggleRef.current?.focus()`; `AlertDialog`
  confirms open with `initialFocus` on the Cancel action (the
  shipped `@octanejs/base-ui` primitive prop); the ConnectDialog
  step swap re-announces via the swapped `aria-label` ("Connect a
  system" → "Pair the connector") (NFR7.3/7.5).
- B. Let the live region announce raw deltas — every `liveTail`
  mutation inside a polite region queues an announcement;
  per-frame chatter at stream cadence is the failure the
  boundary-only pin exists to prevent.
- C. `aria-live="off"` + a timer-driven announcer — drops the
  pinned attribute pair BR10.1 names verbatim and invents a
  cadence no spec row pins.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Two-channel announce on `OxpilotTranscript`: the container. Conductor-decided under standing delegation; ratify at PR review.

## Q5. Performance posture — how is the typing-echo proxy guarded with zero instrumentation?

- A. Structural posture only — no runtime probes: append-only
  render (dedupe O(1) set lookup + `liveTail[id]` extend; the
  full transcript never re-renders per frame), plaintext while
  streaming (zero parse per delta; `renderGfm` once on complete),
  boundary-only announcements, bottom-only autoscroll (scrolled
  up ⇒ the jump-pill renders and ZERO scroll work runs) — the
  NFR3 proxy (< ~100 ms echo @ 500 messages, aspirational) holds
  by construction, not measurement. The CI leg: the extended
  `oxpilot.integration.test.ts` drives a synthetic N-append
  stream asserting keyed row identity is preserved (only the tail
  row's text mutates) and `oxpilot-stream.unit.test.ts` pins
  dedupe; NO `performance.mark`/`PerformanceObserver`/render
  counters ship — NFR6.1's zero-telemetry pin covers
  instrumentation surfaces and an unshipped probe measures
  nothing.
- B. `performance.mark`/`measure` behind a dev flag — dead
  instrumentation on a pinned-silent surface; flag-gated probes
  are a second behavior path the spec never pins.
- C. Real-user latency beacons — the exact telemetry class NFR6.1
  forbids, plus an egress path for untrusted transcript data.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Structural posture only — no runtime probes: append-only. Conductor-decided under standing delegation; ratify at PR review.

## Q6. Secrets-render boundary — how is the pairing-code's shown-once lifetime enforced in code?

- A. Lifetime = ConnectDialog step-2 `useState`:
  `pairing_create{target_name, caps}` resolves `{pairing_id,
  pairing_code, expires_at}` into dialog-local state; step 2
  composes `ox pilot pair <code>` and hands the composed string
  to `CopyCommand` (read-only `Input` + labelled copy —
  components/settings/copy-command.tsrx) — the SOLE render site.
  The code never enters the transcript subtree, never the live
  region (the `Input` carries no announcement role and lives
  inside the dialog), never `console.*`, never
  `localStorage`/`sessionStorage`/cookies, never written to any
  `queryKey` (the `useMutation` result sits in the MutationCache
  only until gcTime and the dialog calls `mutation.reset()` on
  close so it drops with the state — gc-bounded, not persisted),
  never prop-drilled beyond the one `command` prop;
  Esc/`[Done]`/close drops the state and a reopened dialog
  re-mints (no proc returns a code twice — C1's shown-once doc).
  Enforcement is structural plus a grep pin in the coverage test:
  `pairing_code`/`pairingCode` appears only inside
  `connect-dialog.tsrx` and the `CopyCommand` call site
  (NFR2.1/2.4).
- B. Write the pairing response into the query cache for step-2
  retry — cache entries outlive the dialog's lifetime boundary
  and expose the code to unrelated readers; shown-once is a
  lifetime pin, not a visibility preference.
- C. Mask the code after first paint — `CopyCommand` must copy
  the real `ox pilot pair <code>` command verbatim (clipboard =
  displayed string, NFR2.4); masking breaks the pair verb.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Lifetime = ConnectDialog step-2 `useState`:. Conductor-decided under standing delegation; ratify at PR review.

## Q7. Test/observability hooks — what shape do the coverage rows and stream tests take?

- A. Extend the shipped trio verbatim (BR11): the two `oxpilot.*`
  happy-dom rows in `route-coverage.manifest.ts` stay on
  `oxpilot.integration.test.ts` — the file extends to the targets
  section, both dialogs, the four card kinds, flag-off, and the
  queue path, plus the structural grep pins
  (`dangerouslySetInnerHTML` confined to the SEC-2 two-sink
  allowlist, zero other innerHTML-class APIs +
  `createElement("script")`/`eval(`/`new Function(` absent under
  `components/oxpilot/**` + both oxpilot route files,
  `pairing_code` confinement, the verbatim `type:"tool_call"`
  discriminant arm — a `type:"tool"` arm is a pinned defect);
  `browser-coverage.manifest.ts` rebinds `oxpilot-repo-trigger`
  onto `components/oxpilot/new-session-dialog.tsrx` and gains
  subject-keyed rows {`oxpilot-new-session`,
  `oxpilot-connect`} on `oxpilot.browser.test.tsx` — browser rows
  only on gate-discovered surfaces
  (`browser-coverage-check.ts:255-259` orphan-fails undiscovered
  files); the button-only subjects {`oxpilot-session-row`,
  `oxpilot-target-row`, `oxpilot-consent`, `oxpilot-rail-toggle`,
  `oxpilot-diff-card`, `oxpilot-drawer`} are exercised inside the
  happy-dom route rows (corrected at infra-design review —
  recorded narrowing of this option's earlier 8-row browser set);
  `oxpilot-stream.unit.test.ts` covers the `tool_call`/`card`
  union variants, append dedupe, malformed-frame skip, and the
  `es.onerror` probe's fatal-vs-transient split. Test seams are
  `data-testid` + DOM queries only — no exported test hooks, no
  instrumentation deps (NFR6.3/TS-9).
- B. New stack-browser e2e suite for the workspace — the spec
  scopes e2e out; happy-dom + the existing browser project cover
  the pinned rows (rejected at nfr-req Q7-B).
- C. Per-component snapshot tests — `.tsrx` snapshot tooling is
  not the shipped pattern; behavior assertions via
  `@octanejs/testing-library` are (TS-9).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Extend the shipped trio verbatim (BR11): the two `oxpilot.*`. Conductor-decided under standing delegation; ratify at PR review.

## Consolidated Summary Confirmation

- Render boundary: two-phase plaintext→once-GFM through the
  sanitized `MarkdownHtml` seam plus the shipped escape-built
  `contentHtml` diff-highlight sink — a two-sink allowlist grep
  pin keeps every other surface text-node-only (Q1).
- Stream errors: `es.readyState` discriminator — CONNECTING ⇒
  network drop, native retry; CLOSED ⇒ HTTP-denial class, one
  `session_get` names it and `onFatal` surfaces the typed error
  (Q2).
- Card recovery: the 4-read set folds into ONE keyed-remount
  seed; refused `grant_list{session_id}` degrades to the Manage-
  expander path with the claim narrowed on record (Q3).
- A11y: boundary-only announce via `aria-hidden` stream tail +
  `role="status"` announcer; focus via mount-focus, Cancel
  initialFocus, Esc-to-toggle (Q4).
- Perf: structural posture only — no probes; identity-
  preservation asserted in the happy-dom/stream tests (Q5).
- Secrets: pairing-code lifetime is dialog step-2 state →
  `CopyCommand`; grep-pinned confinement (Q6).
- Tests: shipped manifests extended — happy-dom rows,
  subject-keyed browser rows, stream unit tests; no new infra
  (Q7).

[Answer]: Looks correct
