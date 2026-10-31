# Unit Test Instructions — oxpilot-ui (U8)

## Framework & config

- **Vitest 5** inside `apps/web` (`vitest.config.ts`) with three
  projects this unit touches: `unit` (node env,
  `src/**/*.unit.test.ts` + `*.gate.test.ts`), `integration` (happy-dom,
  `fileParallelism: false`, `src/**/*.integration.test.{ts,tsx}`,
  `src/test/setup-integration.ts` + `@testing-library/jest-dom`), and
  `browser` (Playwright Chromium headless, `testTimeout` 30 s,
  `src/**/*.browser.test.{ts,tsx}`).
- **`@octanejs/testing-library`** mounts —
  `renderWithQueryClient`/`mountWithQueryClient`,
  `trackDomErrors`/`expectNoOctaneOverlayInDocument`,
  `pickSelectOptionByTestId` — via `src/test/browser-mount.ts`.
- **Coverage manifests** (`src/test/`): `route-coverage.manifest.ts`
  keeps its two `oxpilot.*` happy-dom rows verbatim — both bound to
  `routes/oxpilot.integration.test.ts`, `route_test_status: covered`.
  `browser-coverage.manifest.ts` rebinds `oxpilot-repo-trigger` to
  `components/oxpilot/new-session-dialog.tsrx` and gains
  `oxpilot-new-session` (same surface) + `oxpilot-connect`
  (`components/oxpilot/connect-dialog.tsrx`) — every row `kind:
  "browser"` bound to `oxpilot.browser.test.tsx`. The gate's discovery
  pass orphan-fails rows on files with no high-risk primitive, so only
  those three surfaces get rows.
- **Rust side (flagged amendments only)**: `cargo-nextest` —
  `crates/oxidean-db/tests/dialect_oxpilot.rs` (3-dialect schema +
  store legs, `DATABASE_URL`-gated), `crates/oxidean-api/tests/oxpilot.rs`
  (proc-integration through `test_app` + `proc_dispatcher`), colocated
  `oxidean-api-oxpilot` unit tests (`procedures_list_matches_dispatch`
  etc.).
- **Generated client**: `make rpc-gen` then `make rpc-sync-check` —
  the drift gate guards `packages/api-client/src/index.ts`.

## Tests that exist (and which extend)

| File | Exists today | This unit |
|---|---|---|
| `src/lib/oxpilot-stream.unit.test.ts` | shipped (U1 — `parseSseBuffer`, `createAppendDedupe`, `postOxpilotChat` POST/stream sequences) | **extended** — frame-union + `readyState` assertions below |
| `src/routes/oxpilot.integration.test.ts` | shipped (U1 — flag-off, enabled index + `[+ New]`, workspace `useOxpilotChat` POST + `transcript.append`, `turn_active` 409 denial) | **extended** — every surface + grep-pin assertion below |
| `src/routes/oxpilot.browser.test.tsx` | shipped (U1 — `oxpilot-repo-trigger` Select portal DOM-race) | **extended** — rebound subject + two new subjects |
| `crates/oxidean-db/tests/dialect_oxpilot.rs` | shipped (session/message/queue/settings legs ×3 dialects) | **extended** — `computer_use_target_id` legs (amendment #1) |
| `crates/oxidean-api/tests/oxpilot.rs` | shipped (status/session/message/queue/turn procs through `test_app`) | **extended** — the three flagged amendments' proc coverage |
| `crates/oxidean-api-oxpilot` colocated (`procedures_list_matches_dispatch`, `all_procs_are_session_only`) | shipped | auto-covers `oxpilot.queue_delete` by source-scan once the dispatch arm + `PROCEDURES` entry land |

No new test files — the manifest rows bind to these exact files and the
gate orphan-fails unbound additions.

## What each file must cover (assertions)

### `src/lib/oxpilot-stream.unit.test.ts` (extended)

- `type:"tool_call"` frames parse into the tool_call variant VERBATIM —
  carrying `call_id`/`call_index`/`tool`/`status`/`args_summary?`/
  `output_summary?`; a `type:"tool"` frame stays on the catch-all arm
  and never produces a tool row (the REL-13 discriminant pin).
- `type:"card"` frames (`card_kind`, `ref_id`) parse into the card
  variant; an unknown `card_kind` stays a frame (never throws, never
  renders raw).
- `OxpilotMessage.tool_calls` merge shape — a `snapshot` message
  carrying `tool_calls` round-trips through the parser.
- `createAppendDedupe` keeps `"{message_id}:{seq}"` semantics; non-append
  frames (`tool_call`, `card`, `terminal`) pass through unconditionally.
- `openOxpilotStream` `es.onerror` split: `es.readyState === CONNECTING`
  ⇒ **zero** `sessionGet` calls (native retry owns the drop);
  `es.readyState === CLOSED` ⇒ exactly ONE `sessionGet` → `onFatal(code)`
  — assert `not_found`, `oxpilot.disabled`, an unrelated code, and a
  probe throw each terminate on `onFatal` (never a silent hang, never a
  second probe).
- malformed `data:` lines still skip without throwing (existing pins
  keep passing).

### `src/routes/oxpilot.integration.test.ts` (extended — happy-dom)

- **Index**: Sessions section rows newest-first (`title`, `owner/repo ·
  branch`, status badge text, relative time); `focus-within` `[Delete]`
  → `OxpilotConfirmDialog` → `sessionDelete` → invalidate + toast.
  `Computer use` section at `#targets` — lifecycle→text labels incl.
  `pending_consent` ⇒ "needs consent" + `[Connect]` re-offer that
  opens the Manage expander; expander `grantList{target_id}` pending
  connect grants `[Allow]`/`[Deny]` → `grantRespond`; `[Disconnect]`/
  `[Revoke]` confirms; empty-state CTA; section-scoped
  loading/empty/error states.
- **`NewSessionDialog`**: `[+ New session]` opens; repo `Select`
  required (submit refuses empty); branch `Select` via `repo.refs`
  (defaults `default_branch`); goal omitted when blank; `target_id`
  sent only on a `connected` pick (non-connected refused client-side);
  success → invalidate `oxpilotSessionList` + navigate
  `/oxpilot/$sessionId`; error → inline notice, draft preserved.
- **`ConnectDialog`**: step-1 gating (target_name + ≥1 cap +
  understanding checkbox all gate `[Continue]`); `pairingCreate` →
  step-2 `CopyCommand` verb `ox pilot pair <code>`; `targetList` poll
  matched on `pairing_id` bounded by `expires_at` →
  `grantList{target_id}` → `grantRespond{allow:true}` → connected +
  `[Done]`; timeout ⇒ error + retry; Esc abandons; `pairing_code`
  never inside the transcript subtree (SEC-14 assertion).
- **`OxpilotConfirmDialog`**: Cancel `initialFocus`; verbatim confirm
  label; the four destructive rides (`sessionDelete`,
  `targetDisconnect`, `targetRevoke`, `diffApply`).
- **Workspace**: rail collapse `aria-expanded` + local-state toggle;
  `SessionsRail` footer chips (target name + status text incl.
  `connected·busy` from in-flight exec/fs cards); transcript
  `aria-live="polite"` boundary-only announce; autoscroll
  at-bottom-only + "jump to latest" pill.
- **Card dispatch**: `tool_call` `call_id`-keyed create/update
  (running→done/error; `<code>` summaries verbatim); `card{consent}` →
  `grantGet` → three-button `grantRespond` (`scope:"once"`/
  `scope:"session"`/`allow:false` verbatim payloads) → freeze +
  outcome line; `grant_not_pending`/`not_found` ⇒ refetch + freeze with
  resolved outcome; `card{diff}` → `diffGet` → stats + `[Show patch]`
  `<pre>` + `[Create branch + PR]` → confirm → `diffApply` → `pr_ref`
  `AppLink` + toast (never auto-applied); `card{queued}` → `queueList`
  → `QueuedRow`; unknown `card_kind` ⇒ generic "update your client"
  notice card.
- **Queue path**: live-turn send → `messageQueue{session_id, content,
  client_msg_id}` → `QueuedRow` ("queued — sends next"); `{null,null}`
  receipt ⇒ no row; `409 turn_active` ⇒ typed banner, NEVER
  auto-enqueue; inline cancel → `queueDelete` → row drops on
  `queueList` drain (amendment #3).
- **Flag-off**: `status{enabled:false}` on BOTH routes (M7
  `OxpilotFlagOff` + `flagOffReason`, zero controls) AND mid-session
  `oxpilot.disabled` ⇒ "OxPilot is disabled" + back link; `not_found`
  seed ⇒ "Session not found" + back link.
- **Stream honesty**: `terminal{error}` ⇒ notice + `[Retry]` re-attach;
  `terminal{aborted}` ⇒ "Reply stopped." + `[Retry]`; composer never
  disabled; `draft` survives error; `[Stop]` → `turnCancel` when
  present.
- **Mount/reseed**: the pinned 4-read set — `fetchOxpilotSnapshot` +
  `sessionGet` + `grantList{session_id}` + `diffList{session_id}` —
  fires on mount/remount/fatal-dismiss; pending grant/proposal rows
  reconstruct pending-or-frozen consent/diff cards (3-read degraded set
  iff the grant amendment is refused); first post-attach `snapshot`
  skipped, LATER `snapshot` ⇒ `liveTail` clear + `setSeed(version+1)`
  keyed remount.
- **SEC-2 grep pin**: `dangerouslySetInnerHTML` under
  `components/oxpilot/**` + both oxpilot routes ONLY inside
  `diff-card.tsrx`/`context-drawer.tsrx`; ZERO
  `innerHTML`/`outerHTML`/`insertAdjacentHTML`/`document.write`/
  `createElement("script")`/`eval(`/`new Function(` in the same scope —
  the grep pattern is the API names, never a bare `script` substring.
- **SEC-14 grep pin**: `pairing_code`/`pairingCode` confined to
  `connect-dialog.tsrx` + the `CopyCommand` call site.
- **Zero-telemetry pin**: no `performance.mark`/`PerformanceObserver`/
  beacon/`console` metrics in `components/oxpilot/**` + the two routes.

### `src/routes/oxpilot.browser.test.tsx` (extended — Chromium subjects)

- `oxpilot-repo-trigger` (rebound): `OxpilotNewSessionDialog` mounts,
  repo `Select` opens + `pickSelectOptionByTestId` picks inside the
  `Dialog` — `expectNoOctaneOverlayInDocument` + no
  `insertBefore`/removeChild console noise (the shipped DOM-race pin,
  now inside the dialog).
- `oxpilot-new-session`: mount → open → `Select` repo pick +
  `RadioGroup` target pick inside the `Dialog` → assert
  `data-browser-subject`/`data-browser-assert` markers; no overlay.
- `oxpilot-connect`: mount → step-1 `Checkbox` toggles + `Input` +
  `[Continue]` gating → step-2 swap (label re-announce +
  `data-browser-subject="oxpilot-connect"` swap to `oxpilot-connect-step2`);
  no overlay.

### Rust side (flagged amendments only)

- `crates/oxidean-db/tests/dialect_oxpilot.rs`: `computer_use_target_id`
  column round-trip (insert/select) on all three dialect shapes;
  `SESSION_SELECT_{PG,MYSQL,SQLITE}` column parity; NULL default on
  rows predating the column; the migration filename-parity
  `migrations_dir_in_sync` covers `0090` automatically.
- `crates/oxidean-api/tests/oxpilot.rs`: `session_create{target_id}`
  accepted + persisted (`computer_use_target_id` on `session_get` AND
  `session_list`); foreign/absent target ⇒ `not_found`; non-`connected`
  lifecycle ⇒ `bad_input`; `grant_list{session_id}` narrows to that
  session's grants only; `queue_delete{session_id, queued_id}` owned
  rows ⇒ `{deleted:true}` + absent from `queue_list`, foreign/absent
  ⇒ `not_found`, `require_verified`/`require_enabled` gates hold.
- `oxidean-api-oxpilot` `procedures_list_matches_dispatch` —
  `oxpilot.queue_delete` in `PROCEDURES` + the dispatch arm (auto).
- `make rpc-gen` + `make rpc-sync-check` — generated `target_id?`,
  `computer_use_target_id`, `session_id?`, `OxpilotQueueDelete*`
  surface committed in the same change.

## Exact unit-scoped commands

```bash
# web projects (from repo root)
cd apps/web && bunx vitest run --project unit src/lib/oxpilot-stream.unit.test.ts
cd apps/web && bunx vitest run --project integration src/routes/oxpilot.integration.test.ts
cd apps/web && bunx vitest run --project browser src/routes/oxpilot.browser.test.tsx

# lint + format gates (required before any web commit)
make web-lint && make web-format-check

# coverage manifest gates
make route-coverage-check && make browser-coverage-check

# server side — flagged amendments
cargo nextest run -p oxidean-api-oxpilot
cd crates/oxidean-db && mkdir -p var && \
  DATABASE_URL="sqlite:./var/dialect_oxpilot.db" \
  cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)' -j 1
cargo nextest run -p oxidean-api --test oxpilot
cargo check -p oxidean-api-oxpilot -p oxidean-api-core -p oxidean-db --tests

# generated-client drift gate
make rpc-gen && make rpc-sync-check
```

## How CI runs them (lane map — `ci.yml`)

| Lane | What it runs for this unit |
|---|---|
| `web-octane` (ci.yml:299) | `bun run test` = vitest `unit` + `integration` — the extended stream unit test + the extended happy-dom file incl. all grep pins; `make web-lint` (type-aware oxlint + `check-octane-dom-races` + anchors/semicolons/route-sync/`pending-before-notfound` repo checks); `format:check` (oxfmt + prettier); turbo build; `web-render-check` |
| `web-browser` (ci.yml:363) | the `browser` project — the three `oxpilot-*` subjects replayed across the 5-shard matrix |
| `route-coverage` (ci.yml:410) | `make route-coverage-check` — the two kept `oxpilot.*` happy-dom rows |
| `browser-coverage` (ci.yml:442) | `make browser-coverage-check` + `ui-coverage-change.contract.sh` — the discovery pass + orphan-fail on the rebind + two new rows |
| `coverage-weighted` (ci.yml:474) | `make coverage-web` collects v8 for the extended web tests; 0.70 composite floor |
| `dead-code` (ci.yml:329) | `bunx knip` — unused exports/files under `components/oxpilot/` fail |
| `rpc-sync` (ci.yml:639) | `make rpc-sync-check` — the amendment fields' generated surface |
| `db-dialects` (ci.yml:855, `-E` filterset :902) | the `dialect_oxpilot` leg replays `0090` + the round-trip legs on postgres/mysql/sqlite |
| `api-rust` / `rust-build` shards | the shared nextest archive compiles + runs `oxidean-api-oxpilot`, `oxidean-api-core`, `oxidean-db`, `oxidean-api` colocated + integration tests |
| `commit-lint` (ci.yml:114) | SemVer-prefixed subjects — `feat(web)`/`feat(oxidean-web)` |

No new CI jobs — the existing lanes + the ruleset's required checks
already cover every file this unit touches (infrastructure-design).

## Coverage targets

- Strategy: **standard** — five to eight tests per component; unit +
  integration on key boundaries; the change-type minimums from
  `docs/CODE_PRACTICES.md` (high-risk UI ⇒ Chromium row + manifest;
  dialect-touching DB ⇒ `dialect_*` test; RPC contract ⇒ integration).
- Scope floor (feature): 80% line coverage on web unit/integration
  (v8), CI green before merge — floors are inputs, not targets to
  weaken.

## Mocking/stubbing

- `vi.mock("@/lib/api-client")` — the shipped pattern; every proc the
  UI calls joins the mock surface: `status`, `sessionList/Create/Get/
  Delete`, `messageQueue`, `queueList`, `queueDelete`, `turnCancel`,
  `pairingCreate`, `targetList/Get/Disconnect/Revoke`,
  `grantList/Get/Respond`, `diffList/Get/Apply`.
- `vi.stubGlobal("EventSource", FakeES)` — the shipped fake gains
  `readyState` + `onerror` control for the REL-3 arms.
- `vi.stubGlobal("fetch", …)` for the snapshot/`postOxpilotChat` SSE
  pair (the shipped `sseResponse` helper shape).
- No live server, connector, or Docker in the web lanes;
  `pairing_code` fixture strings stay test-local (never the transcript
  assertion's positive case).

## Test data

- Fixtures mirror the wire DTOs: `OxpilotSession` incl.
  `computer_use_target_id`; `OxpilotTargetRow` with `lifecycle`
  variants (`connected`/`pending_consent`/`pairing`/`disconnected`/
  `revoked`) + `pairing_id`/`expires_at`; `GrantView` `state`
  transitions (`pending`/`approved`/`denied`/`expired`) +
  `granted_by`/`revocation`; `ProposalRow` + `ProposedDiff`
  (`pr_open`/`merged`/`failed:*`); queued rows; the `{null,null}`
  queue receipt.
- Non-2xx envelopes `{error:{code,message}}` for the WF4.3/WF12
  vocabulary: `turn_active`, `budget_exceeded`,
  `session_active_turn_absent`, `grant_not_pending`, `pairing_expired`,
  `diff_not_pending`, `apply_failed:*`, `not_found`, `oxpilot.disabled`.

## Skip rules

- Manifest `skip` rows are the only sanctioned skip — **zero**
  `it.skip`/`test.skip`/`#[ignore]` anywhere in the new surface
  (the affirmed testing posture says so verbatim).
- **No new browser rows for button-only surfaces** — the
  browser-coverage gate's discovery pass scans `*.tsrx` for
  `Select`/`Checkbox`/`RadioGroup`/`SelectPortal`/`DialogPortal`/
  `Toolbar`/`Popover`/`Accordion`/`Tabs`/`form.Subscribe` and
  orphan-fails rows on undiscovered files
  (`browser-coverage-check.ts:255-259`); consent card, rail collapse,
  diff-card cluster, session/target rows, and the drawer stay inside
  the happy-dom route rows.
- **No stack-browser/e2e additions** — the `e2e-stack`/`e2e-stack-browser`
  projects stay `E2E_STACK`-gated and out of this unit's scope.
- **No new CI jobs** — the existing lane map covers everything; do not
  add lanes, matrices, or ruleset edits.
- **Do not weaken or skip required tests** — `postOxpilotChat` and
  `fetchOxpilotSnapshot` remain the unit-test seams (the production
  send path is `useOxpilotChat`'s fetcher); every existing assertion in
  the three shipped files keeps passing unchanged except where the
  design deliberately rebinds a surface (the `oxpilot-repo-trigger`
  surface path).
