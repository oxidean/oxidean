# Security Requirements — oxpilot-ui (U8)

Consumes: `construction/oxpilot-ui/functional-design/rules.md`
(BR1–BR11), `functional-design/functional-spec.md` (WF1–WF12,
Negative paths, Accessibility, Contract deltas),
`functional-design/frontend-components.md`,
`nfr-requirements-questions.md`,
`inception/requirements-analysis/requirements.md` (NFR1, NFR2),
`inception/contract-design/contract-summary.md` (C1, C2),
`inception/domain-design/components.md` §OxpilotWeb,
`codekb/oxidean-app/technology-stack.md`.

The UI holds no enforcement authority — [domain] "the UI enforces
nothing itself" — so its security posture is a render/custody
posture: every server-, model-, or target-supplied string is
adversary-authored until it reaches the DOM as text, and the only
secret the client ever holds (the pairing code) is shown-once. The
server re-checks everything; the UI's duties are to never become an
injection vector, to never fabricate a state the server would deny,
and to never lie to the user about what happened.

## Render injection boundary

| id | requirement | basis |
|----|-------------|-------|
| NFR1.1 | Assistant text renders `whitespace-pre-wrap` PLAINTEXT while `streaming`; on `complete` it passes the shipped `renderGfm` path ONCE (unified + remark/rehype incl. sanitize — [codekb]). No `innerHTML`-class insertion exists anywhere in the unit OUTSIDE the single sanitized `MarkdownHtml`/`dangerouslySetInnerHTML` seam — a hostile model can emit markup but it is never interpreted mid-stream | BR5.2, FD Q8-A, WF4 |
| NFR1.2 | `ToolCallCard` fields — tool name, `args_summary`, `output_summary` — render verbatim as `<code>`/text, NEVER markdown and NEVER HTML; status renders as text ("running"/"done"/"failed") | BR5.1, FD Q8-A, frontend-components |
| NFR1.3 | Diff patches render through `parseUnifiedDiffLines`/`highlightDiffLines` — read-only, syntax-marked, escaped; `[Show patch]` is a `<pre>` text toggle. A patch body's contents can never reach the DOM as markup | BR5.3, WF10 |
| NFR1.4 | User/server-supplied display strings — target `label` (the `OxpilotTargetView` wire field), session `title`, grant `args_summary`, `QueuedRow` content, grant `capability` — render as text only; row/expander/card templates never interpolate them into markup or raw HTML attributes | BR1.3, WF6, WF7 |
| NFR1.5 | `card`/`tool_call` frames carry refs + summaries only: `card{card_kind, ref_id}` resolves card CONTENT through owner-checked C1 reads (`grant_get`/`diff_get`/`queue_list`) — a frame cannot inject card body, and an unknown `card_kind` renders the generic "update your client" notice card, never raw frame fields | BR5.1, WF6, FD Q7-A |

## Session & route authorization surface

| id | requirement | basis |
|----|-------------|-------|
| NFR1.6 | Both routes gate on `auth.me` — unauthenticated or `auth.me` error ⇒ `usePageRedirect` to `/login?returnTo=/oxpilot[/$session]`; any authenticated user is admitted (FR1.6 — no role gate exists or is added) | WF1.1, WF3.1, BR1.1 |
| NFR1.7 | `not_found` on `session_get`/snapshot ⇒ error notice + back link — foreign and absent sessions are indistinguishable (anti-enumeration surface, AC5.5.4's UI half); the `es.onerror` probe treats `not_found` as FATAL — `EventSource.close()`, since native reconnect would retry a foreign session forever | BR9.3, WF12, BR3.3 |
| NFR1.8 | `oxpilot.disabled` anywhere ⇒ the flag-off surface with ZERO interactive controls (AC5.1.1) — a mid-session proc-level `oxpilot.disabled` error swaps to the same surface; the UI never leaves affordances rendered that the server would only deny | WF1.3, WF12, Negative paths |
| NFR1.9 | Every action rides session-bound C1 procs or the C2 routes verbatim — no call carries a user-id/session-id field the client fabricates; `sessionId` derives from `matchPath` on the route; `client_msg_id` is `crypto.randomUUID()` per send — an opaque idempotency key, never an identity | WF3.2, WF4.1, C1/C2 |

## Consent & mutation surface

| id | requirement | basis |
|----|-------------|-------|
| NFR1.10 | Consent cards render `grant_get`-resolved state only: the three buttons fire `grant_respond{grant_id, allow:true, scope:"once"|"session"}` or `{allow:false}` verbatim — NO "always" affordance exists (FR5.2); `state!=="pending"` ⇒ buttons inert + the frozen outcome line; `grant_not_pending`/`not_found` on respond ⇒ refetch + freeze with the resolved outcome — no optimistic lie | BR6.1, BR6.2, BR5.4, WF7 |
| NFR1.11 | Destructive actions ride the shared M5 `AlertDialog` — explicit confirm with the destructive label verbatim, Cancel taking initial focus; `diff_apply` fires ONLY through the confirm path — no auto-apply, no implicit trigger, no second path in code | BR6.3, BR5.3, WF9, WF10.4 |
| NFR1.12 | The connect dialog mints via `pairing_create{target_name, caps}` only — the client never holds a `target_id` it did not read back from `target_list`; connect consent applies through the pending-grant `grant_respond{grant_id, allow:true}` path when the grant materializes, never a self-minted grant | BR7.2, WF8.3 |

## Secret hygiene on screen (NFR2 rows)

| id | requirement | basis |
|----|-------------|-------|
| NFR2.1 | The minted `pairing_code` renders ONCE in step 2's read-only `CopyCommand` field (`ox pilot pair <code>` verbatim) — never re-fetched, never written to browser storage, never rendered into the transcript live region, never logged (console or otherwise), dropped on dialog close | AC4.1.4, BR7.3, WF8.2 |
| NFR2.2 | No secret-class value renders anywhere: C1 payloads carry none by contract (provider key material is `key_set`/`key_last4` on the `admin.oxpilot.*` surface — outside this unit); connector verifier material, env values, and redaction-masked originals never appear | C1 notes, FR5.4 |
| NFR2.3 | Transcript and card content renders the server's already-redacted strings verbatim — the UI performs no un-redaction and adds no client-side raw-arg echo: consent cards and tool cards show the bounded `args_summary` copies, never raw command text | WF6, WF7, C6/C7 |
| NFR2.4 | Clipboard payloads are exactly the displayed strings: `ox pilot pair <code>` via `CopyCommand`, the raw unified-diff patch via `[Show patch]` `<pre>`, the `pr_ref` link — never a different payload than shown, never hidden extras | WF8.2, AC2.3.1, WF10.3 |
| NFR2.5 | No oxpilot state persists to browser storage — all UI state is route-local memory (TanStack Query cache + `useState` per [ds-map] state ownership); a remount re-seeds from the server, so nothing sensitive outlives the tab | [ds-map], BR3.2, frontend-components state ownership |

## Threat considerations

- Assistant text is the primary injection vector: model output is
  adversary-class content reaching a DOM. The plaintext-while-
  streaming pin (NFR1.1) means the hostile string is never parsed
  mid-stream; the completed-render path is the shipped sanitized
  `renderGfm` toolchain — the same boundary the repo already trusts
  for markdown. No second HTML channel is introduced.
- Consent-card spoofing reduces to a ref, not a payload: a
  `card{consent, ref_id}` names a grant — the rendered capability
  and args come from the owner-checked `grant_get` read, so a
  crafted frame cannot fabricate consent text; worst case it names
  a grant that resolves to a real pending row or renders a
  resolved/absent state (NFR1.5, NFR1.10).
- Target labels and session titles are stored user content rendered
  back to the same user — still treated as untrusted text
  (NFR1.4): a stored `<script>` in a label must survive to the DOM
  only as a literal string.
- Link egress is closed: `pr_ref` renders via `AppLink` to the
  internal `/{owner}/{repo}/pull/{n}` route; the flag-off doc link
  points at `docs/CONFIGURATION.md`. No server-supplied string ever
  becomes an arbitrary external `href` — no open-redirect or
  phishing surface (WF10.4, WF1.3).
- CSRF/XSRF on the POST+GET pair adds no new surface: the SSE send/
  stream routes ride the app's existing same-origin session-auth
  transport under `/api/` — the UI introduces no cross-origin
  calls, no credentials-in-URL, and no new auth material in
  headers; typed non-2xx responses render per WF4.3 (C2).
- The pairing code is the one secret the client holds: its entire
  lifetime is dialog-step memory — mint→display→close — with no
  transcript write, no storage write, no log write (NFR2.1). The
  "shown once" claim is a render-time property the code makes
  structural, not a promise.
- Anti-enumeration is preserved render-side: `not_found` covers
  foreign and absent uniformly on the workspace route (NFR1.7) —
  the UI never surfaces "exists but not yours" as a distinct
  state.
