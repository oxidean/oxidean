### Code-Generation Architecture Review — oxpilot-ui — iteration 2

**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 2
**Reviewed source:** reviewed-source-058eedd7797e.tsv + post-iteration deltas (reviewed-source-1ced11bde5ab.tsv)

Verification was performed against the working tree at `ddbfd944` — every R-finding was re-verified against implementation, not the fix descriptions. Gates executed locally: `browser-coverage-check` PASS (inventory mode, discovered=58 browser=42 stack-browser=19 skip-only=3), `route-coverage-check` PASS, `oxpilot-stream.unit.test.ts` 20/20, `oxpilot.integration.test.ts` 52/52, `oxpilot.browser.test.tsx` 3/3 under Chromium, `make web-lint` clean (632 files, 0 errors), `make web-format-check` clean, and the three new Rust proc tests pass (`cargo test -p oxidean-api --test oxpilot` — 3/3).

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
| R-10 | Minor | apps/web/src/routes/oxpilot.$session.tsrx:207-211 | `bad_input` arm + `session_id` stamping landed, but the scope-verify admits `== null` — an unscoped response still renders foreign cards | Filter pushed cards to `g.session_id === sessionId` exactly | Partial |
| R-11 | Minor | apps/web/src/routes/oxpilot.$session.tsrx | `oxpilot.disabled` send lacked flag-off swap | Route denial to fatal swap | Resolved |
| R-12 | Minor | apps/web/src/routes/oxpilot.tsrx | `[New session]` rendered in flag-off state | Gate on `enabled` | Resolved |
| R-13 | Major | code-summary.md:135-136 | Summary still carries the falsified "stack-level rows for the browser-coverage subjects" claim verbatim; test descriptions stale vs the landed 52-case suite | Correct the summary's test/coverage claims | Unresolved |
| R-14 | Nit | new-session-dialog.tsrx | Stale `target_id` docstring | Update docstring | Resolved |

### New findings

| ID | Severity | Location | Finding | Required action |
|----|----------|----------|---------|-----------------|
| N-01 | Major | transcript.tsrx:134-142, oxpilot.$session.tsrx:344-346,442,539 | `queue_delete` success never removes the rendered queued row — `queuedRows` is local state with no `queueList` observer and the server emits no `card{queued}` frame on delete; the cancel affordance appears inert and a re-click produces a `not_found` banner | Remove the row from local state on mutation success (or subscribe `queueList`) so the cancel is visibly honored |
| N-02 | Major | target-row.tsrx:61,80-124,288-305 + oxpilot.tsrx:248-251,277-293,97-103 | Disconnect/Revoke double-confirm + double-fire: the row owns confirm+mutation AND the parent's post-success callback opens a second route-level confirm that re-issues the proc on accept; toast fires only via the phantom path; tests ratify the phantom | Keep the row-owned confirm; remove the route-level second confirm so accept doesn't re-issue the proc |
| N-03 | Minor | sessions-rail.tsrx:8-21,32-38; oxpilot.$session.tsrx:609-615 | Rail footer chip never renders the pinned `connected·idle`/`connected·busy` derivation — no busy prop, no exec/fs derivation | Wire the busy state into the rail chip |
| N-04 | Minor | queued-row.tsrx:22-25; transcript.tsrx:271 | Queued-row copy deviates from the pinned "queued — sends next" / "still queued — send after retry" guidance; `stillQueued` binds to `busy` not the post-error state | Match pinned copy + bind to post-error |
| N-05 | Nit | transcript.tsrx:252-254 | Unknown `card_kind` notice deviates from the pinned "update your client" copy | Match pinned notice text |
| N-06 | Nit | integration.test.ts:1973-1987 | SEC-2 pin allowlists a sink-free file (`context-drawer.tsrx`) and never scans the `contentHtml` producer | Extend the pin to scan the producer path |

### Detail

### R-01 — Resolved (was Blocker)

The manifest now binds exactly one row per subject: `components/oxpilot/new-session-dialog.tsrx` → subject `new-session-dialog` and `components/oxpilot/connect-dialog.tsrx` → subject `connect-dialog`, both to `apps/web/src/routes/oxpilot.browser.test.tsx` (`apps/web/src/test/browser-coverage.manifest.ts`). `make browser-coverage-check` prints `mode=inventory-only discovered=58 browser=42 stack-browser=19 skip-only=3` then `PASS`. The test file imports both components (`oxpilot.browser.test.tsx:45-46`) so import markers resolve, and carries three real open-dialog subjects: repo Select pick inside the mounted `NewSessionDialog` (135-151), Select+RadioGroup inside the same dialog (153-182), and the full `ConnectDialog` step-1 gating → `pairingCreate` → step-2 `CopyCommand` flow (184-236) — including the pairing-code confinement assertion `cmd.value === "ox pilot pair PAIR-TEST-42"` (229). All 3 pass under Chromium.

### R-02 — Resolved

Every mandated arm landed in `apps/web/src/lib/oxpilot-stream.unit.test.ts`: `type:"tool_call"` parses into the variant verbatim (114), a `type:"tool"` frame stays on the catch-all arm (143), `type:"card"` frames carry `card_kind`/`ref_id` (159), an unknown `card_kind` remains a frame without throwing (174), the `tool_calls` snapshot merge shape round-trips (183), non-append frames pass the dedupe unconditionally (230-258), and `openOxpilotStream` splits `readyState`: CONNECTING → zero `sessionGet` probes and no fatal (333-354), CLOSED → exactly one probe → `onFatal(code)` for `not_found`/`oxpilot.disabled`/unrelated codes (355-370), a throwing probe → `onFatal("stream_error")` (371-383), frames route through `onmessage` (384+). 20/20 tests pass.

### R-03 — Resolved (with two residual product deviations now tracked as N-03/N-04)

The mandated extension landed comprehensively — `apps/web/src/routes/oxpilot.integration.test.ts` now covers flag-off both routes (219, 621, 1090 mid-session swap), the enabled index (241), mount/reseed with the pinned 4-read set (503, 527 wholesale reseed, 568 consent/diff reconstruction, 585 `grantList{session_id}` refusal degrade), `not_found` seed (602), `tool_call` running→done (644), consent dispatch verbatim payloads (678, 708, 725), diff card + confirm (753), unknown-kind notice (810), `card{queued}` (823), `message_queue` verbatim + `{null,null}` no fabricated row + denial keeps draft (859, 922, 955), queue_delete (983), foreign terminal notices + reseed (1025, 1047), budget env names (1059), Stop/turn_cancel (1118), CLOSED-stream single-probe mapping (1153, 1173), rail chips + drawer (1196, 1232), session rows + delete confirm + error section (1322, 1363), the full target-row surface incl. connect expander (1380-1523), disconnect/revoke confirms (1542-1601), the new-session dialog incl. `target_id`-only-on-connected (1604-1698, 1733), connect dialog lifecycle (1771-1905), and the three source pins SEC-2/SEC-14/zero-telemetry (1950-2007). 52/52 pass. Minor unasserted residuals (goal-omitted-when-blank payload, jump-to-latest scroll) are acceptable happy-dom limits; the two deviations that are *implementation* gaps rather than test gaps are filed as N-03/N-04.

### R-04 — Resolved

Commit `ddbfd944` landed all three proc-level tests in `crates/oxidean-api/tests/oxpilot.rs` through the real RPC dispatcher (`test_app`/`test_app_with_gateway` at 1724-1742 installs the DB-backed `OxpilotConnectorGateway`):

- `session_create_target_id_binds_persists_and_validates` (1801): `pending_consent` target → `rpc.bad_input` (1815-1817); absent and foreign ids → `not_found` identically (1822-1830); approved `connected` target binds and persists through BOTH `session_get` AND `session_list` reads (1834-1874).
- `grant_list_session_id_narrows_and_denies` (1883): per-session narrowing returns exactly the bound grant (1927-1951); the connect grant (`session_id NULL`) is excluded from the scoped read but present in the unscoped read, proving the filter is real (1956-1973); absent and foreign session ids → `not_found` (1978-2001).
- `queue_delete_drains_and_denies_foreign_or_absent` (2011): absent `queued_id` and absent session → `not_found` (2041-2060); foreign session → `not_found` with the owner's row untouched (2064-2075); owned delete → `{deleted:true}` + `queue_list` drained (2078-2097); stale re-delete → `not_found` (2100-2109).

Verified passing: `cargo test -p oxidean-api --test oxpilot` → `3 passed; 0 failed` (0.84s). These are real proc-level tests through the request path — not the DB-dialect legs the iteration-1 review correctly discounted.

### R-05 — Resolved

`oxpilot.$session.tsrx:683-690` renders a `budget_exceeded` code branch naming `OXIDEAN_OXPILOT_USER_BUDGET` and `OXIDEAN_OXPILOT_INSTANCE_BUDGET` plus `docs/CONFIGURATION.md`. Both env names are real (`crates/oxidean-api-oxpilot/src/settings.rs:79-80`, documented at `docs/CONFIGURATION.md:401-402`). Integration assertion at `oxpilot.integration.test.ts:1059`.

### R-06 — Resolved

`OxpilotComposer.submit` clears the text only when the callback result `!== false` (`composer.tsrx` — `const ok = props.turnLive ? await props.onQueue(content) : await props.onSend(content); if (ok !== false) setText("")`). Both producers return `false` on failure: `sendIdle` returns `false` on a thrown send and on a fetcher-reported failure via `sendFailRef` — which `useOxpilotChat`'s `onFetchError` hook fires synchronously inside the run (`oxpilot.$session.tsrx:329-334, 493-515`; `lib/oxpilot-chat.tsrx:129-149`); `sendQueued` returns `false` on any mutation error (548-553). Draft preservation asserted at `oxpilot.integration.test.ts:955, 1733`.

### R-07 — Resolved

No `<main>` element exists anywhere under `apps/web/src/routes/oxpilot*.tsrx` or `apps/web/src/components/oxpilot/` — `AppPage` supplies the single landmark (`app-page.tsrx:80`). The transcript is `<section aria-label="Transcript">` (`transcript.tsrx:176`).

### R-08 — Resolved

`SessionsRail` handles Escape inside the `nav` and focuses `[data-testid="oxpilot-rail-toggle"]` (`sessions-rail.tsrx:42-49`, toggle at 59) — the same contract the context drawer holds on `oxpilot-context-toggle` (`context-drawer.tsrx:22-24, 48-51, 70`).

### R-09 — Resolved

`confirmTargetAction` awaits with `.catch(() => {})` so a failed disconnect/revoke lands only in the typed `targetError` surface (`oxpilot.tsrx:97-103`). The duplicated confirmation architecture this handler belongs to is itself a new defect — see N-02.

### R-10 — Partial

Landed: `bad_input`/`oxpilot.bad_input` added to `isGrantAmendmentRefusal` (`oxpilot.$session.tsrx:96-106`), `GrantView.session_id` is declared `Option<String>` and stamped from the stored row (`oxidean-api-core/src/oxpilot.rs:663-666`; `gateway.rs:286`), and the server-side scoped read is strict (`list_grants_for_session` filters `session_id = $2` — NULL never matches, `oxidean-db/src/oxpilot.rs:4030-4046`).

Residual defect — the card-push filter is fail-open in exactly the scenario the finding named:

```ts
for (const g of grants ?? []) {
  if (g.session_id == null || g.session_id === sessionId) {   // oxpilot.$session.tsrx:207-211
    pushCard("consent", g.id);
  }
}
```

`GrantListReq` deserializes with `#[serde(default)]` and no `deny_unknown_fields` (`connector/procs.rs:101-110`), so a pre-amendment server silently ignores `{session_id}` and returns the unscoped owner list — whose grants serialize WITHOUT any `session_id` field. `undefined == null` passes the guard, so every pending grant the user owns — including other sessions' action grants — still pushes a consent card in this transcript, and the comment's claim that "those cards never render here" (203-206) is false for that case. The `== null` arm admits nothing legitimate: a correct amended server can never emit `session_id: null` from this read (the store filters `session_id = $2` strictly, and connect grants carry NULL by design — `oxidean-db/src/oxpilot.rs:4030-4035`). The required action said filter to `g.session_id === sessionId`; as implemented the defense holds only against a hypothetical amended-but-unscoped server, not the unamended one the matcher+comment are built for. Required: drop the `== null` arm (or check `"session_id" in g` — explicit-null connect grants can never legitimately appear here either way).

### R-11 — Resolved

`sendCode === "oxpilot.disabled"` feeds `effectiveFatal`, which renders `OxpilotFlagOff` — the same surface as mount/fatal (`oxpilot.$session.tsrx:588-603`). Mid-session disable asserted at `oxpilot.integration.test.ts:1090`.

### R-12 — Resolved

`[New session]` renders only inside the `enabled` branch (`oxpilot.tsrx:110-119`); flag-off renders `OxpilotFlagOff` with no live controls (128-129).

### R-13 — Unresolved

`code-summary.md` was never corrected. The verbatim falsified line stands: "`e2e/stack/oxpilot.stack.test.ts` — stack-level rows for the browser-coverage subjects" (135-136) — the stack test remains the single U1 skeleton slice (`describe("stack e2e: /oxpilot skeleton slice")`, one test, `oxpilot.stack.test.ts:48-141`) and cannot supply component-level browser coverage. The file's only commit is its creation (`07aa795e`); neither `c633a68b` nor `b1a3c4b7` touched it. The integration/browser descriptions are also stale relative to what landed — lines 129-134 describe roughly five integration cases where 52 now exist, and the summary still makes no statement either way about the server proc-level coverage that `ddbfd944` has since landed. Required: rewrite the Tests-and-coverage section to describe what the files actually assert.

### R-14 — Resolved

`new-session-dialog.tsrx:54-57` now documents the landed semantics — the amendment "binds the session to the target atomically; the server validates owner + `connected` lifecycle" — the stale "(extra field is ignored)" wording is gone.

### N-01 — Major (new): `queue_delete` success leaves the cancelled row rendered

The transcript's `queueDelM` `onSuccess` only calls `invalidateOxpilotQueries(qc)` (`transcript.tsrx:134-142`). But the rendered set is local state — `queuedRows` in `OxpilotChatSurface` (`oxpilot.$session.tsrx:344-346`) — fed exclusively by `reseed()` (228), `card{queued}` frames (441-444), and the post-send refresh (538-542). `oxpilotQueueListQuery` is only ever `fetchQuery`'d imperatively (442, 539): there is NO active `useQuery` observer anywhere, so the invalidation refetches nothing and `setQueuedRows` is never called on delete. The server cannot close the gap either — `queue_delete` emits no `card{queued}` frame (`session.rs:404-412`; the emit at 361-372 is enqueue-only). Net behavior: the × deletes the row server-side, yet the row stays rendered indefinitely (until an unrelated enqueue emits a queued card or a reseed happens); clicking it again answers `not_found` and surfaces "Could not remove the queued message" (`transcript.tsrx:139-141`) — an error banner for a delete that already succeeded. The integration test's drain mechanism cannot occur in production: it emits a second `card{queued}` frame by hand so the refetch fires (integration.test.ts:1017-1021), ratifying an event the server never sends on delete. Required: on `queue_delete` success, remove the row from `queuedRows` (lift an `onCancelled` callback to `setQueuedRows`) or `fetchQuery(queueList)`+`setQueuedRows` — staying server-authoritative either way — and fix the test to assert the row drops without a synthetic frame.

### N-02 — Major (new): target disconnect/revoke double-confirm, double-fire, no primary-path toast

`OxpilotTargetRow` owns the full destructive flow: local `confirm` state, its own two `OxpilotConfirmDialog`s, and the `targetDisconnect`/`targetRevoke` mutations (`target-row.tsrx:61, 80-124, 288-305`). On success it calls `props.onDisconnect()`/`props.onRevoke()` — which the row's own docstring defines as "the parent's cache-refresh hooks, fired after success" (46-47), matching the session-row pattern (`OxpilotSessionRow onDelete` → `refresh() + toastSuccess`, `oxpilot.tsrx:167-173`). The parent instead wires those callbacks to open a SECOND route-level `OxpilotConfirmDialog` (`oxpilot.tsrx:248-251, 277-293`) whose accept arm `confirmTargetAction` fires the same proc again (97-103). Resulting UX: user confirms "Disconnect desk-box?" in the row dialog → the proc fires silently (no toast — `toastSuccess` exists only on the parent's mutation) → an identical dialog re-appears → confirming it re-sends the destructive request (idempotent server-side, but a real duplicate) and only then toasts; cancelling leaves a phantom prompt after an action that already ran. The parent's comment (63-65) claims "the shared M5 confirm + the procs live here" — directly contradicting the row's ownership and making the route-level mutations reachable only through the phantom path. The integration tests ratify the defect rather than flag it ("The parent's cache-refresh hook re-offers the same confirm ride", `oxpilot.integration.test.ts:1562-1573, 1592-1601`). Required: pick one owner — pass `onDisconnect`/`onRevoke` as `() => { refresh(); toastSuccess(...) }` (mirroring `onDelete`) and delete the route-level confirm+mutations, or strip the row's dialogs/mutations and delegate to the parent.

### N-03 — Minor (new): rail footer chip lacks the pinned `connected·idle`/`connected·busy` derivation

`targetStatusLabel` maps `connected` → bare `"connected"` (`sessions-rail.tsrx:8-21`), `SessionsRail` takes no busy prop (32-38), and the workspace passes bare `targets` with no derivation from in-flight exec/fs tool-call cards (`oxpilot.$session.tsrx:609-615`). AC4.5.1 and the mandated test assertion ("footer chips … incl. `connected·busy` from in-flight exec/fs cards", unit-test-instructions.md workspace section) pin the derivation; `OxpilotTargetRow.busy` implements the label correctly (`target-row.tsrx:23-36`) but no caller supplies it on either surface.

### N-04 — Minor (new): queued-row pinned copy/trigger deviation

The spec pins "`◐` + content + 'queued — sends next'; after a terminal error 'still queued — send after retry'" (functional-spec.md). The row renders `"queued" | "still queued" — {content}` — the guidance copy never appears (`queued-row.tsrx:22-25`) — and `stillQueued` binds to `props.busy` (a live turn) rather than the post-terminal-error state the spec pins (`transcript.tsrx:271`).

### N-05 — Nit (new): unknown-`card_kind` notice deviates from pinned copy

The spec pins a generic "update your client" notice for unknown card kinds (functional-design-questions.md:270). Implementation renders "Unsupported card kind "X" — skipped." (`transcript.tsrx:252-254`). The `card_kind` token is a closed protocol enum — not raw payload — so this is not a leak, but the pinned affordance text differs and no "update your client" guidance exists anywhere.

### N-06 — Nit (new): SEC-2 pin allowlist over-broad; producer outside scanned scope

The SEC-2 source pin allowlists `components/oxpilot/context-drawer.tsrx` for `dangerouslySetInnerHTML` (`oxpilot.integration.test.ts:1974-1977`), but the file contains no such sink — a future raw sink there would pass the pin. The `contentHtml` producer the one real sink consumes (`lib/lazy-diff-rows.tsrx` → `lib/highlight-diff.tsrx`) sits outside the scanned `oxpilotSources()` set (1954-1971), so the escaping seam itself is un-pinned. Acceptable in spirit — the pipeline is a shared, previously-reviewed seam — but the allowlist should match the files that actually carry sinks and ideally include the producer.

### Verification coverage map

| Area | Result |
|------|--------|
| C2 stream contract (snapshot/append/dedupe/terminal/readyState) | Verified — `oxpilot-stream.ts` + unit pins 20/20 |
| Mount/reseed 4-read set + BR7.4 degrade | Verified — `oxpilot.$session.tsrx:165-240`, degrade tested at 585 |
| Plaintext streaming vs completed `MarkdownBody` | Verified — `transcript.tsrx:203-217` plaintext+caret; `:216` MarkdownBody |
| Consent freeze + Deny-during-Allow | Verified — `consent-card.tsrx:58-60, 99-130` (Deny intentionally ungated) |
| Destructive confirms | Partially broken — N-02 (double-fire); session delete + diff apply correct |
| Pairing-code confinement (SEC-14) | Verified — `pairing_code` only in `connect-dialog.tsrx`, only inside `CopyCommand` value; browser test 223-229 + grep pin 1989-1999 |
| Generated client parity | Verified — `OxpilotQueueDelete{Request,Response}`, `GrantListRequest.session_id`, `computer_use_target_id` present (`packages/api-client/src/index.ts:5397-5521, 5329, 7405`) |
| Server amendments | Verified — `session_create` target validation (`session.rs`), `grant_list{session_id}` narrowing (`procs.rs:116-129`, `gateway.rs:962-980`, `oxpilot.rs:4036+`), `queue_delete` scoping (`session.rs:404-412`); proc tests pass |
| `window_day` rename | Verified — `oxidean-db/src/oxpilot.rs:852-859, 983-984, 1047-1057` |
| Vite plugin exports-map fix | Verified — `tanstack-ai-octane.ts` resolves bare imports via `import.meta.resolve` from the real package dir |
| Web gates | `web-lint` 0 errors; `web-format-check` clean; browser/route coverage checks PASS |

### Merge verdict

NOT-READY. Two iteration-1 findings remain open (R-10 Partial — the scope-verify fails open for the unamended-server case it was built for; R-13 Unresolved — the falsified summary claim is verbatim unchanged), and two new Major defects (N-01 invisible queue cancel, N-02 double-confirm/double-fire on target destructive actions) need fixes before merge. N-03/N-04 are Minor spec deviations; N-05/N-06 are advisory Nits.
