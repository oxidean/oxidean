### Code-Generation Architecture Review — oxpilot-ui — iteration 3

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 3
**Reviewed source:** reviewed-source-058eedd7797e.tsv + post-iteration deltas

Verification was performed against the working tree at `7712f32b` — every iter-2 claim was re-verified against implementation, not the commit message. Gates executed locally: `oxpilot.integration.test.ts` 52/52 PASS, `oxpilot-stream.unit.test.ts` 20/20 PASS, `make web-lint` clean (632 files, 0 errors), `make web-format-check` clean, `route-coverage-check` PASS (discovered=84 happy-dom=78 stack-browser=22), `browser-coverage-check` PASS (inventory mode, discovered=58 browser=42 stack-browser=19 skip-only=3).

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| - | - | - | No findings | No action required | Resolved |

### Prior findings

| ID | Severity | Location | Finding | Required action | Status |
|----|----------|----------|---------|-----------------|--------|
| R-01 | Critical | apps/web/src/test/browser-coverage.manifest.ts, oxpilot.browser.test.tsx | Manifest duplicated rows/phantom subjects + closed-portal test | Consolidate rows; add real open-dialog subjects | Resolved |
| R-02 | Major | apps/web/src/lib/oxpilot-stream.unit.test.ts | U8 stream unit-test extension missing | Land instructed assertions | Resolved |
| R-03 | Major | apps/web/src/routes/oxpilot.integration.test.ts | Integration-test extension missing | Land instructed cases + grep pins | Resolved |
| R-04 | Major | crates/oxidean-api/tests/oxpilot.rs | No proc-level amendment tests | Add the three proc tests | Resolved |
| R-05 | Major | apps/web/src/routes/oxpilot.$session.tsrx | `budget_exceeded` lacked env-naming notice | Add code-keyed branch | Resolved |
| R-06 | Major | apps/web/src/components/oxpilot/composer.tsrx | Failed send destroyed draft | Propagate send success | Resolved |
| R-07 | Major | transcript.tsrx, oxpilot.tsrx | `<main>` nested in `<main>` | Swap to section/div | Resolved |
| R-08 | Minor | sessions-rail.tsrx | Rail Esc missing focus return | Add onKeydown Escape | Resolved |
| R-09 | Minor | oxpilot.tsrx | `confirmTargetAction` unhandled rejection | Catch the rejection | Resolved |
| R-10 | Minor | apps/web/src/routes/oxpilot.$session.tsrx:207-213 | `bad_input` arm + `session_id` stamping landed, but the scope-verify admitted `== null` — an unscoped response still rendered foreign cards | Filter pushed cards to `g.session_id === sessionId` exactly | Resolved |
| R-11 | Minor | apps/web/src/routes/oxpilot.$session.tsrx | `oxpilot.disabled` send lacked flag-off swap | Route denial to fatal swap | Resolved |
| R-12 | Minor | apps/web/src/routes/oxpilot.tsrx | `[New session]` rendered in flag-off state | Gate on `enabled` | Resolved |
| R-13 | Major | code-summary.md:129-159 | Summary carried the falsified "stack-level rows for the browser-coverage subjects" claim verbatim; test descriptions stale vs the landed suite | Correct the summary's test/coverage claims | Resolved |
| R-14 | Nit | new-session-dialog.tsrx | Stale `target_id` docstring | Update docstring | Resolved |
| N-01 | Major | transcript.tsrx:138-149; oxpilot.$session.tsrx:699-701 | `queue_delete` success never removed the rendered queued row — no `queueList` observer and no server delete frame; cancel looked inert, re-click produced `not_found` | Remove the row from local state on mutation success; assert drop without a synthetic frame | Resolved |
| N-02 | Major | oxpilot.tsrx (route) vs target-row.tsrx:80-124,288-305 | Disconnect/Revoke double-confirm + double-fire: row-owned confirm+mutation AND a phantom route-level confirm re-issuing the proc on accept; toast only via the phantom path | Keep the row-owned confirm; remove the route-level second confirm | Resolved |
| N-03 | Minor | sessions-rail.tsrx:6,21,90-97; oxpilot.$session.tsrx:605-618,634 | Rail footer chip never rendered the pinned `connected·idle`/`connected·busy` derivation — no busy prop, no exec/fs derivation | Wire the busy state into the rail chip | Resolved |
| N-04 | Minor | queued-row.tsrx:21-30; transcript.tsrx:280; oxpilot.$session.tsrx:698 | Queued-row copy deviated from the pinned "queued — sends next" / "still queued — send after retry"; `stillQueued` bound to `busy` not the post-error state | Match pinned copy + bind to post-error | Resolved |
| N-05 | Nit | transcript.tsrx:259-263 | Unknown `card_kind` notice deviated from the pinned "update your client" copy | Match pinned notice text | Resolved |
| N-06 | Nit | oxpilot.integration.test.ts:1958-1981 | SEC-2 pin allowlisted a sink-free file (`context-drawer.tsrx`) and never scanned the `contentHtml` producer | Extend the pin to scan the producer path | Resolved |

### New findings

| ID | Severity | Location | Finding | Required action |
|----|----------|----------|---------|-----------------|
| N-07 | Minor | components/oxpilot/context-drawer.tsrx:6-20,120-122 | The drawer keeps the pre-N-03 `targetStatusLabel` map — `connected` renders bare (outside the pinned `connected·idle`/`connected·busy` label set, functional-spec.md:127,647) with no busy derivation; the same bound target now shows `connected·idle`/`connected·busy` in the rail footer and bare `connected` in the drawer on the same screen, and the comment "Same lifecycle → text map as the target row / rail footer" is false | Reuse `lifecycleLabel` (import from `target-row.tsrx`) and feed the busy bit — `OxpilotChatSurface` already computes `busyTargets`; pass `busyTargets.has(t.id)` through `ContextDrawer` (or the resolved label) at oxpilot.$session.tsrx:724-730; fix the parity comment either way |
| N-08 | Minor | oxpilot.integration.test.ts:1196-1230 | The mandated rail-chip busy leg is unasserted — no test binds a target and emits a running `exec_shell`/`fs_*` `tool_call` frame to assert `connected·busy` (unit-test-instructions.md:101-103 pins "status text incl. `connected·busy` from in-flight exec/fs cards"); `connected·idle` is only asserted on the index row (1436), so the busyTargets derivation could be deleted without tripping the suite | Extend the rail-chip test (or add one): sessionGet with `computer_use_target_id`, emit a running `exec_shell` `tool_call` frame → assert `connected·busy` on the chip; emit its `done` transition → assert `connected·idle` |
| N-09 | Nit | oxpilot.$session.tsrx:441-452,541-546; transcript.tsrx:140-145 | The local row-drop can be overwritten by a stale read: a `card{queued}`-triggered (or post-send) `fetchQuery(queueList)` started before the delete resolves afterwards with a pre-delete snapshot and re-adds the cancelled row until the next card/reseed — a re-click then answers `not_found` → the "Could not remove" banner N-01 set out to kill. Narrow server-timing window; the post-delete invalidate covers cached/later reads (10 s `staleTime`) but not the in-flight one | Advisory: keep a `deletedIds` set until the next server-sourced refresh (merge-forward), or gate the fetch resolution on a monotonically increasing read version; also fix the stale test title at 983 ("row drops on the queue_list drain" — the drop is local now) |

### Detail

### R-10 — Resolved

`oxpilot.$session.tsrx:207-213` now admits only `g.session_id === sessionId` — the `== null` arm is gone and the comment names the threat. Verified the generated seam: `OxpilotGrantView.session_id: string | null` (`packages/api-client/src/index.ts:5507`), so an amended server stamps the bound session id and an unamended server (serde-default `GrantListRequest`, ignores `{session_id}`) returns rows whose field is absent → `undefined !== sessionId` → excluded; a hypothetical explicit-null connect grant is excluded too. `isGrantAmendmentRefusal` keeps `bad_input`/`oxpilot.bad_input` (96-106) for the degrade branch. The exact-match lands the defense the matcher+comment were built for.

### R-13 — Resolved

`code-summary.md:129-159` rewritten against the landed suite, every claim re-verified: 52 integration `it(` blocks (`grep -cE '^\s*it\('` = 52; ran green 52/52), 3 Chromium mounts (`oxpilot.browser.test.tsx` = 3), 20 stream cases (17 `it(` + one 4-case loop at 349-369; ran green 20/20), and the three proc tests exist at `crates/oxidean-api/tests/oxpilot.rs:1800,1882,2011`. The falsified "stack-level rows for the browser-coverage subjects" line is gone — the stack file is now honestly described as "the U1 skeleton slice". The manifest/route-coverage and deferred-build notes match reality.

### N-01 — Resolved

`queueDelM.onSuccess(_data, vars)` calls `props.onQueuedDelete?.(vars.queued_id)` before `invalidateOxpilotQueries` (`transcript.tsrx:140-145`); the workspace passes a `setQueuedRows` filter on `q.id` (`oxpilot.$session.tsrx:699-701`). `vars` is `OxpilotQueueDeleteRequest` (`{session_id, queued_id:string}` — `oxpilot-queries.ts:350-361`, `packages/api-client/src/index.ts:5397-5401`). Test "inline cancel rides queue_delete" (983-1023) now drops the synthetic second `card{queued}` frame and asserts the row is gone after the real delete — 52/52 green. Residual edge filed as N-09.

### N-02 — Resolved

The phantom ride is fully excised from `oxpilot.tsrx`: `useMutation`, `OxpilotConfirmDialog`, `oxpilotTargetDisconnectMutation`/`oxpilotTargetRevokeMutation` imports removed; `confirm`/`targetError` state, `disconnectM`/`revokeM`, `confirmTargetAction`, the `targetError` banner block, and the route-level dialog all deleted (file is 231 lines, no dead imports — oxlint clean). `OxpilotTargetRow` owns confirm+mutation end-to-end (`target-row.tsrx:61,80-124,288-305`); parent callbacks are `refresh() + toastSuccess(...)` (194-201), matching the `OxpilotSessionRow onDelete` pattern (131-134). Tests assert single-fire: `toHaveBeenCalledTimes(1)` + `queryByRole("alertdialog")` absent post-accept (1559-1568, 1584-1592). Minor note: `disconnectM.onSuccess` inside the row also calls `invalidateOxpilotQueries` before `props.onDisconnect()` → `refresh()` invalidates again — idempotent, harmless.

### N-03 — Resolved

`lifecycleLabel` is exported (`target-row.tsrx:23`) and reused by the rail (`sessions-rail.tsrx:6,92-95`) with `busyTargets?: ReadonlySet<string>`; the workspace derives it in `useMemo` from `toolCalls` — any `status==="running"` `exec_shell`/`fs_read`/`fs_write` call adds the bound target (`oxpilot.$session.tsrx:605-618`), passed at 634. `toolCalls` gets a new `Map` identity on every `tool_call` frame and the server emits a final-status frame per call (`runtime.rs:607-618`), so the chip clears correctly. One bound target per session ⇒ the Set-of-one shape is right; an unbound session yields an empty set. `connected` → `connected·idle`/`connected·busy` matches interaction-spec.md:54-55 and functional-spec.md:127,402. The mandated busy-leg *test* remains absent — filed N-08; the sibling surface kept the stale map — filed N-07.

### N-04 — Resolved

`queued-row.tsrx:22-29` renders `{content}` + ` — queued — sends next` / ` — still queued — send after retry` — the pinned guidance now ships verbatim (functional-spec.md:294-295). `stillQueued={props.turnFailed === true}` (`transcript.tsrx:280`) bound to `turnFailed={sendBanner != null}` (`oxpilot.$session.tsrx:698`); `sendBanner = sendError ?? chatError` covers queue denials (e.g. `session_active_turn_absent` keeps the draft + banner), send failures, and own-turn terminal errors (`RUN_ERROR` → `chat.error` via `oxpilot-chat.tsrx:97-102`). Acceptable residual: a *foreign* `terminal{error}` surfaces the page-level `turnNotice` + reseed instead, so rows show "sends next" after the remount — defensible since the retry affordance is the notice's `[Retry]` and a new turn drains the queue.

### N-05 — Resolved

`transcript.tsrx:259-263` renders `Unsupported card kind “${card.card_kind}” — update your client.` — pinned guidance landed (functional-spec.md:288, functional-design-questions.md:270); integration matcher updated (818) and green.

### N-06 — Resolved

The SEC-2 allowlist is now `{"components/oxpilot/diff-card.tsrx"}` only (1971-1981) — the sink-free `context-drawer.tsrx` entry is dropped — and `lib/highlight-diff.ts` is in `oxpilotSources()` (1958-1967). Verified the producer is clean: `highlight-diff.ts` builds `contentHtml` exclusively through `escapeHtml` (`&<>"`) + Shiki `<span style>` wrappers — zero `innerHTML`/`eval`/template-sink matches. `lazy-diff-rows.tsrx` (the consumer seam) constructs no HTML itself. `diff-card.tsrx:94` remains the sole `dangerouslySetInnerHTML` sink, consuming only `useLazyDiffRows` output.

### Test updates — verified

Disconnect/revoke single-ride assertions (1562-1568, 1587-1592: `toHaveBeenCalledTimes(1)` + no `alertdialog`), `queuedRowEl` rewritten to content+`/queued/` on `role="status"` (851-857 — no other workspace `role="status"` can false-match: rail chips carry lifecycle labels, card errors carry "Could not load…", the unknown-kind notice ends "client"), the unknown-card matcher (818), and the queue-delete test drops the synthetic second `card{queued}` frame (1017-1022). The `{null,null}`-receipt negative was updated to `not.toContain("merged — queued")` — correctly targets the new `{content} — queued` copy (952).

### N-07 — Minor (new): drawer kept the pre-N-03 label map

`context-drawer.tsrx:7-20` holds a verbatim copy of the `targetStatusLabel` the rail just deleted — `connected → "connected"` bare — and the drawer's docstring claims "Same lifecycle → text map as the target row / rail footer", now false. Bare `connected` is outside the pinned label set (`connected·idle`/`connected·busy`/`offline`/`needs consent` — functional-spec.md:127,647) and the drawer can never show busy. The same bound target renders `connected·busy` in the rail and `connected` in the drawer simultaneously (both visible in the workspace — rail always expanded, drawer one click away). The drawer test's `toHaveTextContent("connected")` (1257) passes either way — substring match — so swapping in `lifecycleLabel` breaks nothing.

### N-08 — Minor (new): mandated `connected·busy` assertion never landed

unit-test-instructions.md:101-103 pins the rail footer assertion "status text incl. `connected·busy` from in-flight exec/fs cards". The rail test (1196-1230) asserts names + `needs consent` + `●2 targets` + collapse only; `connected·idle` is asserted once on the *index* row (1436) and `connected·busy` nowhere (grep confirms zero occurrences in tests). The derivation added for N-03 is real and verified by reading code + the server's running→done frame contract — but it is entirely unexercised: deleting `busyTargets` tomorrow leaves the suite green. Fix is small: seed `sessionGet` with `computer_use_target_id`, emit a `tool_call{exec_shell,running}` frame, assert the chip flips to `connected·busy`, emit the done frame, assert `connected·idle`.

### N-09 — Nit (new): stale in-flight `queueList` read can resurrect a deleted row

The N-01 fix makes `queuedRows` local state divergent from the query cache. The two imperative refreshers — the `card{queued}` handler (`oxpilot.$session.tsrx:444-452`) and the post-send refresh (541-546) — can be in-flight *before* `queue_delete` commits and resolve *after* `onQueuedDelete` filters the row, overwriting `queuedRows` with a pre-delete snapshot: the cancelled row re-appears until the next card/reseed, and a re-click answers `not_found` → "Could not remove the queued message" — the exact UX N-01 targeted, in a much narrower window. The post-success `invalidateOxpilotQueries` (whole `["oxpilot"]` prefix) protects cached and later reads — `oxpilotQueueListQuery` has `staleTime: 10_000` (`oxpilot-queries.ts:174`) — but cannot cancel the in-flight fetch. Advisory fix: hold `deletedIds` until the next server-sourced `setQueuedRows`, or sequence reads with a version counter. Cosmetic tag-along: the test title at 983 still says "row drops on the queue_list drain". (Adjacent pre-existing note, not new: `sendQueued`'s post-mint `fetchQuery` can no-op on a fresh cache and briefly hide a just-minted row — unchanged since iter-2.)

### Verification coverage map

| Area | Result |
|------|--------|
| Grant scope-verify (R-10) | Verified — exact `=== sessionId`; unamended/explicit-null/foreign all excluded |
| Queue cancel (N-01) | Verified — local drop + invalidate; residual race = N-09 |
| Destructive confirms (N-02) | Verified — row-owned single ride; session delete + diff apply unchanged |
| Rail busy derivation (N-03) | Wired + code-verified; mandated test leg missing = N-08; drawer kept stale map = N-07 |
| Queued-row copy/trigger (N-04) | Verified — pinned text; `sendBanner` post-error binding |
| Unknown-kind notice (N-05) | Verified — "update your client" verbatim |
| SEC-2 pin (N-06) | Verified — diff-card-only allowlist; producer scanned + clean |
| code-summary claims (R-13) | Verified — 52/20/3/3 counts re-counted and re-run |
| Web gates | `web-lint` 0 errors; `web-format-check` clean; both coverage checks PASS; integration 52/52; stream unit 20/20 |

### Merge verdict

READY. All fourteen iteration-1 findings plus all six iteration-2 findings verify Resolved against the actual code at `7712f32b` — including the two previously open items (R-10's fail-open scope-verify and R-13's falsified summary) and the two Majors (N-01 invisible queue cancel, N-02 double-confirm/double-fire). Zero open Critical/Major. Three new advisory findings: N-07 (Minor — the context drawer kept the pre-N-03 bare `connected` map, so the bound target's status text now differs between rail and drawer and the parity comment is false), N-08 (Minor — the mandated `connected·busy` test leg never landed, leaving the new derivation unexercised), and N-09 (Nit — a stale in-flight `queueList` read can briefly resurrect a deleted queued row). None blocks merge; all three are cheap follow-ups.
