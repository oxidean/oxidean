# Security Design — oxpilot-ui (U8)

Consumes: `nfr-requirements/security-requirements.md` (NFR1.1–1.12,
NFR2.1–2.5), `functional-design/rules.md` (BR1–BR11),
`functional-spec.md` (WF1–WF12, Negative paths, Contract deltas),
`frontend-components.md`, `nfr-design-questions.md` (Q1–Q7),
`contract-summary.md` (C1 procs + amendments; C2 frames + non-2xx
set + card-ref table), shipped `apps/web/src` —
`routes/{oxpilot,oxpilot.$session}.tsrx`,
`lib/{oxpilot-stream,oxpilot-chat,oxpilot-queries,markdown,
parse-unified-diff,highlight-diff,lazy-diff-rows,relative-time,
toast,session-queries,nav,route-params}.*`,
`components/repo/markdown-body.tsrx`,
`components/settings/copy-command.tsrx`, `components/ui/*`,
`packages/api-client/src/index.ts` (`Oxpilot*` DTOs + generated
`apiClient.oxpilot.*`).

The UI holds no enforcement authority — [domain] "the UI enforces
nothing itself" — so this design is a render/custody posture: every
server-, model-, or target-supplied string is adversary-authored
until it reaches the DOM as text, and the only secret the client
ever holds (the pairing code) is shown-once. Every mechanism below
is a code shape, not a policy: text-node construction by default,
ONE sanitize-carrying HTML seam (plus the shipped escape-built
`contentHtml` diff-highlight sink — SEC-2's two-sink allowlist),
refs-instead-of-payloads on frames, and frozen honest cards.

## Render-injection boundary (NFR1.1–1.5)

### SEC-1 — Two-phase assistant render (NFR1.1, Q1-A)

```text
assistant row
  status === "streaming":
    <p class="whitespace-pre-wrap break-words">{text + liveTail}</p>
    + "▌" cursor + "Oxpilot is replying…" hint
      — Octane {expr} interpolation writes a TEXT NODE; the hostile
        interval is never parsed, per-delta or otherwise
  status === "complete":
    html = await renderGfm(finalText)          // lib/markdown.ts
    <MarkdownHtml html={html} />               // the sanitized seam —
      // components/repo/markdown-body.tsrx:18-23
      //   dangerouslySetInnerHTML={{ __html: props.html }}
  status === "error" | "aborted":
    plaintext partial + the typed notice/Reply-stopped line —
    never the markup path (BR5.2's once-on-complete pin)
```

`renderGfm`'s pipeline is fixed and sanitize-carrying by
construction: `unified → remarkParse → remarkGfm → remarkGithub
(issue-autolink only) → remarkRehype → rehypeSanitize →
rehypeStringify` (lib/markdown.ts:39-55) — `rehypeSanitize` is an
in-pipeline stage, never an opt-in argument, and the doc comment
pins "Never passes raw HTML through". The boundary rule: NO model
string reaches `MarkdownHtml` without passing `renderGfm` — the
only call site that feeds it in this unit is a completed assistant
row. Streaming NEVER parses (NFR3.2's perf half of the same pin).

### SEC-2 — Seam-closure pin (NFR1.1 enforcement half, Q1-A)

`components/oxpilot/**` and both oxpilot route files carry
`dangerouslySetInnerHTML` at EXACTLY TWO enumerated sinks and
nowhere else — (1) `MarkdownHtml` for completed assistant text
(SEC-1's sanitized seam), (2) `diff-card.tsrx`/`context-drawer.tsrx`
feeding `highlightDiffLines`/`useLazyDiffRows`'s
`HighlightedDiffLine.contentHtml` — the shipped diff-highlight
sink whose content is `escapeHtml`-built upstream
(`diff-patch.tsrx:163` precedent); plus ZERO `innerHTML`,
`outerHTML`, `insertAdjacentHTML`, `document.write`,
`createElement("script")`, `eval(`, or `new Function(` anywhere —
asserted by a grep pin riding the route-coverage happy-dom test
(Q7-A) whose pattern is the API names above (never a bare
`script` substring — `transcript.tsrx` prose would
false-positive). Every other surface in the unit is text-node
construction by template — the boundary is a two-sink allowlist,
not convention.

Pin note: the grep observes by file, not by sink class —
`MarkdownHtml`'s own `dangerouslySetInnerHTML` lives in
`components/repo/` outside the pinned scope, so the "two sinks"
enumeration lands as `dangerouslySetInnerHTML` occurrences in
the two diff-surface files alone; a future shared diff-row
helper would collapse to one occurrence without breaking the
pin's intent (the assertion is "no unvetted sink", not an
occurrence count). The script-channel arms apply to
`components/oxpilot/**` + both route files alike.

Recorded deviation (not an amendment): NFR1.1's "single
sanitized seam" phrasing reads as one sink; this unit carries
two enumerated sinks because the shipped diff-highlight
pipeline's `contentHtml` output can only reach the DOM through
`dangerouslySetInnerHTML`. The NFR's intent — every model/
server string sanitized or text-node, no unvetted sink — is met
by construction; the second sink renders `escapeHtml`-built
highlighter output, never raw strings.

### SEC-3 — Tool-call + card fields verbatim (NFR1.2)

`OxpilotToolCall` renders `call.tool`, `args_summary`,
`output_summary` as `<code>`/text and `status` as text
("running"/"done"/"failed") — never markdown, never HTML. The
same rule covers every card's server strings. These are already
server-bounded + server-redacted summaries (U3/U5's clamps); the
UI adds no render interpretation — verbatim text is the boundary.

### SEC-4 — Diff render path (NFR1.3)

Patch text flows
`parseUnifiedDiffLines` (lib/parse-unified-diff.ts — classification
keeps raw text) → `highlightDiffLines`/`plainHighlightedDiffLines`
(lib/highlight-diff.ts) whose `escapeHtml` masks `& < > "` per
line before any `contentHtml` insertion (`DIFF_HIGHLIGHT_SOFT_MAX_CHARS`
200 000 — plain rows past it) → the lazy-rows renderer rides
`useLazyDiffRows` (lib/lazy-diff-rows.tsrx — viewport-gated Shiki).
`[Show patch]` is a `<pre>` text toggle of the raw unified diff —
copyable, never interpreted. A patch body reaches the DOM only as
escaped text inside these seams; no `MarkdownHtml` involvement
anywhere on the diff path.

### SEC-5 — Untrusted display-string inventory (NFR1.4)

| string | source | render site | boundary |
|---|---|---|---|
| target `label` | `OxpilotTargetView.label` (stored user content) | `OxpilotTargetRow`, rail chips, drawer, consent card `targetLabel`, confirm titles | text node — never markup, never an attribute that parses |
| session `title` | `OxpilotSession.title` | `OxpilotSessionRow` `AppLink` text, `PageHeader` description | text node |
| grant `capability` + `args_summary` | `OxpilotGrantView` (bounded+redacted server copy) | `OxpilotConsentCard`, Manage expander rows | text node |
| queued `content` | `OxpilotQueuedMessage.content` | `OxpilotQueuedRow` | text node |
| tool name / summaries | `OxpilotToolCallCard` | `OxpilotToolCall` | `<code>` text — SEC-3 |
| patch bodies | `OxpilotProposedDiff.files[].patch` | diff card / drawer pane | SEC-4 escaped seams only |
| `pr_ref` | `OxpilotProposedDiff.pr_ref` | card link text + `AppLink href` to `/{owner}/{repo}/pull/{n}` | internal route, link text is the ref |

A stored `<script>` inside `label`/`title` survives to the DOM only
as a literal string — Octane interpolation is the whole mechanism;
templates never concatenate these into markup or raw attributes.

### SEC-6 — Card-ref resolution (NFR1.5, Q3-A)

`card{card_kind, ref_id}` and `tool_call` frames carry refs +
summaries — a frame can never inject card BODY:

| `card_kind` | content read (owner-checked C1 proc) | render |
|---|---|---|
| `consent` | `grantGet{grant_id: ref_id}` | `OxpilotConsentCard` (SEC-11) |
| `diff` | `diffGet{proposal_id: ref_id}` | `OxpilotDiffCard` |
| `queued` | `queueList{session_id}` | `OxpilotQueuedRow` set |
| unknown | none | generic "update your client" notice card — never raw frame fields |

Every content read rides the caller's session through the
generated client — the server's ownership check is the guard; a
crafted frame's worst case is naming a grant/proposal that
resolves to a real row (or a `not_found` that renders the stale-
ref state per reliability-design §REL-8).

## Route + session authorization surface (NFR1.6–1.9)

### SEC-7 — `auth.me` gate (NFR1.6)

Both routes run `useQuery(authSessionQueryOptions())` →
`usePageRedirect("/login?returnTo=/oxpilot[/$session]")` on
`data === null || isError` (the shipped pattern,
oxpilot.$session.tsrx:36-37). Any authenticated user is admitted —
FR1.6, no role gate exists or is added. The `oxpilot.status` probe
is gate-exempt server-side and `retry:false` client-side — the
redirect never depends on it.

### SEC-8 — `not_found` uniformity + fatal close (NFR1.7)

`not_found` on `sessionGet`/snapshot ⇒ error notice + back link —
foreign and absent sessions are indistinguishable on screen
(AC5.5.4's UI half; the UI never renders "exists but not yours" as
a distinct state). The `es.onerror` probe treats `not_found` as
FATAL: `EventSource.close()` + `onFatal(code)` typed surface —
a `readyState===CLOSED` error is an HTTP-denial class that never
re-establishes, and the `session_get` probe names it (Q2-A,
NFR4.3).

### SEC-9 — Flag-off zero controls (NFR1.8)

`oxpilot.disabled` anywhere — the status probe's `enabled===false`
or a mid-session proc-level `oxpilot.disabled` — swaps to
`OxpilotFlagOff`: `EmptyState` naming the reason
(`flag_off`/`misconfigured`/`no_provider` → `flagOffReason`),
`OXIDEAN_OXPILOT_ENABLED`, `OXIDEAN_OXPILOT_PROVIDER`,
`docs/CONFIGURATION.md` link — and ZERO interactive controls
(AC5.1.1). The UI never leaves affordances rendered that the
server would only deny; the swap is total, not a banner over live
controls.

### SEC-10 — Session-bound calls only (NFR1.9)

Every action rides C1 procs or the C2 routes verbatim — no call
carries a user-id/session-id field the client fabricates:
`sessionId` derives from `matchPath("/oxpilot/$session",
pathname)?.session` (route param, not state);
`client_msg_id = crypto.randomUUID()` per send/queue — an opaque
idempotency key, never an identity; the `oxpilot`-namespaced
synthetic session id is a U5 mechanism this surface never sees.
All proc calls go through generated `apiClient.oxpilot.*` — the
typed seam carries no free-form identity fields to populate.

## Consent + mutation surface (NFR1.10–1.12)

### SEC-11 — Consent-card honesty (NFR1.10)

`OxpilotConsentCard` renders `grantGet`-resolved state only:

```text
GrantView{kind, capability, args_summary, state, expires_at}
  + target label via the cached targetList query-layer read
state === "pending":
  [Allow once]           -> grantRespond{grant_id, allow:true, scope:"once"}
  [Allow for this session] -> grantRespond{grant_id, allow:true, scope:"session"}
  [Deny]                 -> grantRespond{grant_id, allow:false}
  — M6 verbatim payload fields; NO "always" affordance exists
    (FR5.2; generated OxpilotGrantRespondRequest.scope?: string —
    the literal vocabulary {once,session} is pinned by the caller,
    not the type)
state !== "pending":
  buttons inert + the frozen outcome line — never clickable, never
  re-rendered as actionable
grant_not_pending / not_found on respond:
  refetch grantGet -> freeze with the RESOLVED outcome —
  no optimistic lie (BR6.2, NFR4.7)
```

Buttons fire the payload verbatim — `scope` strings are literal,
never derived from UI text; the card is `role="group"`
`aria-label="Action request"` with Deny always keyboard-reachable
(NFR7.4).

### SEC-12 — Single confirm path for destructives (NFR1.11)

All four destructives ride the shared M5 `AlertDialog` wrapper
`OxpilotConfirmDialog` — explicit confirm carrying the destructive
label verbatim, Cancel taking `initialFocus`:

| action | proc | after |
|---|---|---|
| Delete session | `sessionDelete{session_id}` | invalidate + toast |
| Revoke target | `targetRevoke{target_id}` | invalidate + toast (ends ALL the target's grants incl. pending — U6 BR3.1) |
| Disconnect target | `targetDisconnect{target_id}` | invalidate + toast |
| Create branch + PR | `diffApply{proposal_id}` | toast + `pr_ref` `AppLink` |

`diffApply` fires ONLY through this confirm path — no auto-apply,
no implicit trigger, no second code path (AC2.2.3's UI half); a
`diff_not_pending`/`apply_failed:*` response ⇒ typed banner, the
card holds prior state (reliability §REL-10).

### SEC-13 — Connect mint boundary (NFR1.12)

`ConnectDialog` mints via `pairingCreate{target_name, caps}` ONLY
— `OxpilotPairingCreateRequest` carries no `target_id` field (the
client never holds a `target_id` it did not read back from
`targetList`). Connect consent applies through the pending-grant
`grantRespond{grant_id, allow:true}` path when the grant
materializes (the step-2 `targetList` poll matches `pairing_id` →
`grantList{target_id}` → respond) — never a self-minted grant.
M4's `local/remote` radio drops — no wire field exists for it
(recorded deviation, not an amendment).

## Pairing-code + secret custody (NFR2.1–2.5)

### SEC-14 — Pairing-code lifetime (NFR2.1, Q6-A)

```text
pairingCreate -> {pairing_id, pairing_code, expires_at}
  -> ConnectDialog step-2 useState (dialog-local, nothing else)
  -> composed "ox pilot pair <code>" -> <CopyCommand command=…/>
       read-only Input + labelled copy — the SOLE render site
  -> Esc / [Done] / close -> state drops; reopened dialog re-mints
     (no proc returns a code twice — C1 shown-once)
NEVER: transcript subtree · live region · console.* ·
  localStorage/sessionStorage/cookies · prop drill
  beyond the one `command` prop · a second render site
QUERY-CACHE NOTE: `pairingCreate` rides `useMutation` (TS-2), so
the MutationCache retains `{pairing_code}` until gcTime — the
dialog calls `mutation.reset()` on close/step-2 exit so the code
is dropped with the dialog state; the cache retention is
gc-bounded, never a rendered or persisted surface.
```

Enforcement = the shape above + the grep pin in the coverage test
(`pairing_code`/`pairingCode` confined to `connect-dialog.tsrx` +
the `CopyCommand` call site — Q6-A). The "shown once" claim is a
render-time property the code makes structural, not a promise.

### SEC-15 — Zero secret-class surface (NFR2.2)

No secret-class value renders anywhere in the unit: C1 payloads
carry none by contract — provider key material is
`key_set`/`key_last4` on the `admin.oxpilot.*` surface (outside
this unit); connector verifier material lives on C3/U7; env
values and redaction-masked originals never appear. The connect
dialog's `expires_at` and `pairing_id` are the only mint-response
fields besides the code, and neither is secret-class.

### SEC-16 — Redacted-verbatim render (NFR2.3)

Transcript and card content render the server's already-redacted
strings verbatim — consent cards and tool cards show the bounded
`args_summary`/`output_summary` copies, never raw command text;
the UI performs no un-redaction and adds no client-side raw-arg
echo (WF6/WF7; C6/C7's bounding happened server-side).

### SEC-17 — Clipboard fidelity (NFR2.4)

Clipboard payloads are exactly the displayed strings: `ox pilot
pair <code>` via `CopyCommand`'s `navigator.clipboard.writeText(
props.command)`, the raw unified-diff patch via the `[Show patch]`
`<pre>` selection, the `pr_ref` link — never a different payload
than shown, never hidden extras appended at copy time.

### SEC-18 — No browser-storage persistence (NFR2.5)

No oxpilot state persists to browser storage: all UI state is
route-local memory — TanStack Query cache + `useState` per the
[ds-map] state-ownership pin; a remount re-seeds from the server
(the 4-read set), so nothing sensitive outlives the tab. No
`localStorage`/`sessionStorage`/indexedDB write exists in the
unit — same grep-pin family as SEC-2/SEC-14.

## Transport posture — POST/SSE and CSRF

- Both C2 endpoints ride the app's existing same-origin
  session-auth transport under `/api/`: `fetch` calls carry
  `credentials:"include"` + `content-type: application/json`
  (oxpilot-stream.ts:97-101, oxpilot-chat.tsrx:46-51); the
  EventSource attaches same-origin to
  `/api/oxpilot/sessions/{id}/chat/stream`. The UI introduces NO
  cross-origin calls, no credentials-in-URL, no auth material in
  headers.
- `content-type: application/json` on the POST means a cross-site
  form post can't produce a valid request without a CORS preflight
  — incidental hardening on top of the session cookie's server-side
  SameSite posture (U1's domain, not re-designed here).
- Non-2xx pre-stream denials return typed JSON, not SSE — the C2
  vocabulary renders per WF4.3 (typed composer banners); a forged
  cross-origin attempt could never reach the streaming branch.

## Threat model → mechanism map

| threat | mechanism |
|---|---|
| Hostile assistant text becomes markup | SEC-1 two-phase render — plaintext while streaming; the ONLY HTML path is `renderGfm`→`MarkdownHtml` with in-pipeline `rehypeSanitize` (NFR1.1) |
| A second injection sink sneaks in | SEC-2 grep pin — `dangerouslySetInnerHTML` confined to the two enumerated sinks, zero other innerHTML-class APIs under `components/oxpilot/**` + both routes (NFR1.1) |
| Tool/card fields interpreted as markup | SEC-3 verbatim `<code>`/text (NFR1.2) |
| Patch body reaches DOM as markup | SEC-4 escaped highlighter + `<pre>` toggle; read-only seams only (NFR1.3) |
| Stored `label`/`title`/grant strings execute | SEC-5 text-node construction inventory (NFR1.4) |
| Crafted `card` frame injects consent text | SEC-6 refs-not-payloads — body via owner-checked `grantGet`; unknown kind ⇒ generic card (NFR1.5) |
| Foreign/absent session enumerable | SEC-8 uniform `not_found` + fatal `es.close()` (NFR1.7) |
| Denied affordances left clickable | SEC-9 flag-off swap is TOTAL — zero controls (NFR1.8) |
| Client fabricates identity/session | SEC-10 route-param sessionId + generated procs + randomUUID-as-key (NFR1.9) |
| Optimistic consent lie | SEC-11 `grantGet`-resolved render; freeze+refetch on `grant_not_pending` (NFR1.10/4.7) |
| `diff_apply` auto-fires | SEC-12 single AlertDialog path — confirm verbatim or nothing (NFR1.11) |
| Self-minted grant/target | SEC-13 `pairingCreate`-only mint; consent via pending-grant respond (NFR1.12) |
| Pairing code leaks to transcript/log/storage | SEC-14 dialog-local lifetime + grep pin (NFR2.1) |
| Server secret reaches the UI | SEC-15 zero secret-class fields on the wire (NFR2.2) |
| UI un-redacts or echoes raw args | SEC-16 redacted-verbatim summaries (NFR2.3) |
| Clipboard smuggles a different payload | SEC-17 copies = displayed strings exactly (NFR2.4) |
| Sensitive state outlives the tab | SEC-18 no browser storage — reseed-from-server posture (NFR2.5) |
| CSRF on POST/SSE | same-origin `/api/` + JSON content-type + cookie auth; no new auth material — U1's transport (above) |
| Open-redirect/phishing via server strings | link egress closed — `pr_ref` → internal `/{owner}/{repo}/pull/{n}` `AppLink`; flag-off doc link is static (NFR1.4 threat note) |
