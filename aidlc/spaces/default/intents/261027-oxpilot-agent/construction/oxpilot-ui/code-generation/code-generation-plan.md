# Code Generation Plan — oxpilot-ui (U8)

Unit: `oxpilot-ui` (primary US1.1 index, US1.2 new-session dialog + bind,
US1.7 target manager surfaces, US2.3 plain patch view, US4.5 status
chips rail + index; contributing US1.3 rail/transcript/drawer + cards,
US1.4 visible error, US1.6 snapshot-resume client half, US1.8 pending
chip, US2.1 diff card, US4.1/US4.2 pairing + connect-grant consent,
US4.3 consent card + visible denial, US4.4 revoke confirm). Implements
the approved functional design (`functional-design/` — WF1–WF12
workflows, BR1–BR11, the `frontend-components.md` component map, and
the three flagged C1 amendments this unit owns), the NFR design
(`nfr-design/` — SEC-1…SEC-18 render/custody boundary, REL-1…REL-15
honest surfaces, the pinned 4-read mount/reseed set, one-poll pairing
window, A11Y-1…A11Y-5 machinery), the NFR requirements
(`nfr-requirements/` — NFR1.x injection boundary, NFR2.x secret
hygiene, NFR3.x per-frame cost, NFR4.x stream lifecycle, NFR5.x/NFR6.x
audit-honesty + zero telemetry, NFR-S1–S7 growth bounds, TS-1–TS-10 +
the NFR7/NFR8 conformance bar this unit owns), and the infrastructure
spec (`infrastructure-design/` — no new jobs/lanes/ruleset edits, the
coverage-manifest rows the gate discovery demands, the U8-owned `0090`
migration record).

Consumes `inception/contract-design/contract-summary.md` C1 (the
`oxpilot.*` proc families the UI calls verbatim — `status`,
`session_{list,create,get,delete}`, `message_queue`, `queue_list`,
`turn_cancel`, `usage_get`, `pairing_create`,
`target_{list,get,disconnect,revoke}`, `grant_{list,get,respond}`,
`diff_{list,get,apply}` — all generated client-side already) plus the
three flagged additive C1 amendments the functional design owns:
`session_create.target_id` + `computer_use_target_id` on the
`session_get`/`session_list` reads backed by
`0090_oxpilot_session_target.sql` (the column U1 deferred and U6's
`0088` never added), `grant_list{session_id}`, and `queue_delete`; and
C2 (the SSE frame vocabulary — `snapshot`/`append`/`tool_call`/`card`/
`terminal`/`ping`, the `type:"tool_call"` VERBATIM discriminant,
`{message_id}:{seq}` dedupe, the 409 `turn_active` forbidden pair, no
`Last-Event-ID`, the card-ref resolution table).

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "team",
  "ordering": "We implement the change first, then write and run the tests the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing expectations), and the full CI suite must be green before the PR can merge.",
  "scope": "feature",
  "test_strategy": "standard",
  "project_type": "brownfield",
  "applicable_notes": [
    {
      "layer": "org",
      "text": "We treat tests as a first-class deliverable in every Bolt. The specific\nmethodology (TDD, BDD, ATDD, or classic test-after) is affirmed at\npractices-discovery and recorded in `team.md` under this heading with explicit\n`Methodology` and `Ordering` fields; Code Generation resolves those fields\nindependently from coverage, tooling, and scope notes.\n\nWhen no posture has been affirmed, our default per scope is:\n- **Methodology**: test-after\n- **Ordering**: implement each applicable testable layer, then write and run\n  that layer's tests.\n- `mvp`, `enterprise`, `feature`, `infra`, `classic` add an 80% line-coverage\n  floor and CI execution before merge.\n- `bugfix`, `security-patch` add a targeted regression for the specific\n  bug/vulnerability and require the existing suite to remain green.\n- `express` uses the Minimal strategy: requirement-driven unit tests (one per\n  requirement, with a happy-path floor per component); existing tests remain\n  green.\n- `poc`, `refactor`, `workshop` add no extra new-test floor and require the\n  existing suite to remain green.\n\nThe active `Test Strategy` still applies in every scope and determines test\nvolume/types. Scope floors are additive; they never reduce or replace the\nselected strategy.\n\nBuild and Test verifies defined coverage floors and affirmed quality targets;\nthey may not be weakened to make a step pass.\n\nAffirm a stricter posture in `team.md` if the team commits to one."
    },
    {
      "layer": "team",
      "text": "- **Methodology**: test-after\n- **Ordering**: We implement the change first, then write and run the tests\n  the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing\n  expectations), and the full CI suite must be green before the PR can merge.\n- **TDD option**: an in-repo `tdd` skill (`.agents/skills/tdd/`) exists and\n  may be invoked deliberately for a unit, but red-first is not the team's\n  mandated posture.\n- **Coverage**: CI enforces a **weighted composite gate at floor 0.70**\n  (`coverage-weighted` job, `COVERAGE_WEIGHTED_FLOOR`, composed by\n  `scripts/coverage-weighted.sh`): web unit 0.25 + web integration 0.40 (v8\n  line coverage) + e2e 0.35 — where the e2e third is a **7-path presence\n  checklist** (`scripts/coverage-e2e-checklist.sh`), not executed coverage;\n  e2e correctness is enforced by the separate `e2e-stack` / `actions-e2e` /\n  `smoke-protocol` job gates. The composite **consumes web numbers only**:\n  Rust coverage is collected (`cargo-llvm-cov`, artifacts uploaded) but\n  **not gated**, so this intent's new VCS crate lands with no enforced\n  coverage signal — collect-only Rust coverage is recorded debt, and we do\n  not invent a new numeric floor for it (the change-type minimums + green CI\n  carry its testing). Cross-dialect execution in CI is limited to\n  `dialect_probe` per dialect (`db-dialects` job): the ~30 `dialect_*` parity\n  files are `DATABASE_URL`-gated and **skip-pass green** in the `api-rust`\n  shards — full parity is local-capable only, also recorded debt. No\n  performance-test infrastructure exists (no benches, no load harness) —\n  recorded debt for NFR work, not a blocker.\n- **UI coverage is change-aware**: route- and browser-coverage manifests mean\n  every new/changed `apps/web` route needs test evidence, and new/changed\n  high-risk interactive UI (Checkbox / RadioGroup / Select / Switch /\n  portals / `form.Subscribe`) needs Chromium `*.browser.test.tsx` proof —\n  fresh skip-only rows fail CI, and `kind: \"browser\"` entries are\n  source-verified (mount, interact, assert markers). Manifest `skip` rows are\n  the only sanctioned skip: zero `#[ignore]` and zero `it.skip`/`test.skip`\n  in the tree.\n- **Tooling**: cargo-nextest for the Rust workspace (`.config/nextest.toml`,\n  `ci` profile: no fail-fast, 90s slow-timeout; one shared nextest archive\n  from `rust-build` feeds the 5-way `api-rust` shards, `db-dialects`, and\n  archive consumers — one compile per run; shards re-run whole on failure —\n  coarse flake tolerance). Vitest 5 for TS with five projects in `apps/web`\n  (`unit` on node, `integration` on happy-dom with `fileParallelism: false`,\n  `browser` on Playwright Chromium, `e2e-stack` and `e2e-stack-browser` gated\n  on `E2E_STACK=1`) plus `@oxidean/api-client`'s own node-env suite.\n  Playwright drives real-Chromium component and full-stack e2e; wiremock\n  supplies outbound-HTTP fixtures; Rust integration tests open real SQLite\n  tempdir DBs; protocol smokes run real `git`, `ssh`, and `git-lfs` client\n  binaries against a live Compose stack (fail-closed in CI under\n  `SMOKE_REQUIRE_STACK=1`).\n- **Local/CI asymmetry**: `make test` runs only nextest + web\n  unit/integration; browser, stack e2e, and smokes are separate targets — a\n  green local `make test` proves far less than a green CI run, and smoke\n  scripts skip-pass locally without Docker but fail-closed under `CI`.\n- **Test types / minimums**: per change type — pure helper → unit;\n  Query/session UI → happy-dom integration; high-risk UI → Chromium browser\n  test + manifest row; auth/RPC contract → Rust integration and/or stack e2e;\n  dialect-touching DB code → `dialect_*` test (noting the CI-execution gap —\n  the probe is the only enforced cross-dialect run). Suites: colocated Rust\n  units plus `crates/*/tests/` integration (107 API test files), web\n  unit/integration/component tests, stack e2e with Mailpit/OIDC/Resend-WorkOS\n  stubs (`make test-e2e-stack`), forge protocol smokes, compose bring-up\n  smoke per dialect, and an Actions pipeline e2e."
    },
    {
      "layer": "project",
      "text": "- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14) \n\n- Prefer Docker for debugging and testing (docker compose stack, make up/smoke, act for CI lanes) over bare-metal equivalents (learned 2026-10-18)"
    }
  ],
  "obligations": {
    "strategy": "standard",
    "strategy_volume": [
      "Five to eight tests per component.",
      "Unit tests plus integration tests for key boundaries.",
      "Add E2E, performance, or security tests when requirements demand them."
    ],
    "scope_floor": [
      "Meet an 80% line-coverage floor.",
      "Run the selected tests in CI before merge."
    ],
    "combination_rule": "Apply every selected-strategy obligation and every scope-floor obligation; neither replaces the other, and a targeted scope regression may add the narrowest necessary test type beyond the strategy default."
  },
  "plan_profile": {
    "methodology": "test-after",
    "runner_step": "Verify the existing test runner/configuration and record the exact unit-scoped command.",
    "runner_ready_before_first_test": true,
    "testable_layers": [
      "Data model / database behavior",
      "Repository / data access",
      "Business logic",
      "API / endpoint",
      "Frontend behavior"
    ],
    "steps": [
      "Project structure and production configuration skeleton.",
      "Verify the existing test runner/configuration and record the exact unit-scoped command.",
      "Data model / database behavior - implement.",
      "Data model / database behavior - write and run its tests after implementation.",
      "Repository / data access - implement.",
      "Repository / data access - write and run its tests after implementation.",
      "Business logic - implement.",
      "Business logic - write and run its tests after implementation.",
      "API / endpoint - implement.",
      "API / endpoint - write and run its tests after implementation.",
      "Frontend behavior - implement.",
      "Frontend behavior - write and run its tests after implementation.",
      "Environment/build configuration.",
      "Documentation and traceability."
    ]
  },
  "input_sha256": "sha256:7ae78f2c2f75e8446714d34b7e23c2369d30af0622bdcafe3bba1f8d40a110e3",
  "contract_sha256": "sha256:2f7938d58ac724d699d8c1731ce63148c2703bdae96beda4ac1aef80f9aaec31"
}
```

## Story-to-code traceability

| Build Order group | Stories / ACs served |
|---|---|
| 1 (server amendments — flagged) | US1.2 (AC1.2.1 optional target pick + bind), AC4.5.x session↔target reads (rail chips/drawer), AC4.3.2/AC2.2.1 remount card recovery (`grant_list{session_id}`), US1.8 inline queue-cancel (`queue_delete`) — the three flagged C1 amendments + `0090` backing column |
| 2 (lib layer) | US1.6 (AC1.6.1 reconnect-resume), C2 frame union (`tool_call`/`card`), BR9.3/REL-3 `es.onerror` probe, every query/mutation seam the surfaces call (WF4, WF6–WF10) |
| 3 (components) | US1.1 rows (AC1.1.x), US1.2 dialog (AC1.2.x), US1.7 target manager (AC1.7.x), US4.5 chips (AC4.5.1), US1.3 transcript/cards (AC1.3.x), US1.4 errors (AC1.4.x), US1.8 queue path (AC1.8.x), US2.1/US2.3 diff card + patch (AC2.1.x/AC2.2.x/AC2.3.1), US4.1/US4.2 connect/pair (AC4.1.x/AC4.2.x), US4.3 consent + visible denial (AC4.3.x), US4.4 revoke (AC4.4.1), M7 flag-off (AC5.1.1) |
| 4 (routes) | FR1.x index composition + auth gate, FR2.x three-pane workspace, WF3 seed/reseed + keyed remount, WF5 attach lifecycle, `/settings/ai` anchor link (AC1.7.2) |
| 5 (manifests) | NFR9 route + browser coverage rows (Q12-A/BR11.1) |
| 6 (tests) | all of the above — per change-type minimums |

## Build Order

Module-grouped file deltas; test-after ordering per the contract — each
layer's tests land with it (the server amendments unblock the picker,
binding reads, and recovery reads the UI layers consume, so they lead).
`*` = new file; everything else is a shipped file amended in place.

- [ ] **Group 0 — Verify test runner.** Confirm the unit-scoped commands
  in `unit-test-instructions.md` run green on the unmodified tree
  (`cd apps/web && bunx vitest run --project unit src/lib/oxpilot-stream.unit.test.ts`,
  `--project integration src/routes/oxpilot.integration.test.ts`,
  `--project browser src/routes/oxpilot.browser.test.tsx`;
  `cargo nextest run -p oxidean-api-oxpilot` for the amendment side).

- [ ] **Group 1 — Server amendments (flagged C1 deltas; additive;
  `crates/` + generated `packages/`).** All three amendments land
  server-side in this unit (Q2 recommendation — the functional design
  owns them; refused arms drop per the recorded degraded paths):
  - `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0090_oxpilot_session_target.sql`*
    ×3 dialects — additive `ALTER TABLE oxpilot_sessions ADD COLUMN
    computer_use_target_id` (nullable, default NULL; `REFERENCES
    oxpilot_targets(id)` — the referent U1's deferral waited for,
    shipped in `0088`; `ON DELETE SET NULL` so a tombstoned target
    never orphans the binding). Append-only like `0085`–`0089`;
    `0087`/`0088` are never edited.
  - `crates/oxidean-db/src/oxpilot.rs` — `OxpilotSessionRow.computer_use_target_id:
    Option<String>` + `map_session!` + `SESSION_SELECT_{PG,MYSQL,SQLITE}`
    column lists; `NewOxpilotSession.computer_use_target_id` +
    `insert_session` column/bind list ×3.
  - `crates/oxidean-api-core/src/oxpilot.rs` — `CreateSession.computer_use_target_id:
    Option<String>` pass-through field; `ConnectorGateway::list_grants`
    signature gains `session_id: Option<&str>`; `SessionService` gains
    `delete_queued(user_id, session_id, queued_id)` (the `queue_delete`
    backing verb).
  - `crates/oxidean-api-oxpilot/src/services.rs` —
    `OxpilotSessionService` create/list impls pass the field through;
    `delete_queued` impl scoped `(session_id, queued_id)` under the
    caller's `user_id` (`not_found` on foreign/absent); redaction of
    `title`/`goal` stays inside the service as shipped.
  - `crates/oxidean-api-oxpilot/src/connector/gateway.rs` —
    `list_grants` impl honors the `session_id` filter alongside
    `target_id` (the `oxpilot_grants.session_id` column exists since
    `0088`).
  - `crates/oxidean-api-oxpilot/src/session.rs` — `CreateRequest` gains
    `#[serde(default)] target_id: Option<String>`; `session_create`
    resolves a supplied id through `gateway().get_target` (owner-checked
    — foreign/absent ⇒ `not_found`) and refuses a non-`connected`
    lifecycle with `bad_input`; the shared `session_json` view emits
    `"computer_use_target_id"` so `session_get` and `session_list`
    carry it identically; new `queue_delete{session_id, queued_id}`
    USER proc beside `queue_list` — `require_enabled` →
    `require_verified` → owned `get_session` → `svc.delete_queued` →
    `{deleted: true}` (`not_found` on absent/foreign rows).
  - `crates/oxidean-api-oxpilot/src/dispatch.rs` + `lib.rs` — the
    `"oxpilot.queue_delete"` dispatch arm + `PROCEDURES` entry
    (`procedures_list_matches_dispatch` covers it by source-scan;
    `all_procs_are_session_only` keeps it `SessionOnly`).
  - `crates/oxidean-api/src/bin/rpc_gen.rs` — the template emits
    `packages/api-client/src/index.ts` verbatim, so the oxpilot block is
    amended in the template AND the regenerated output stays consistent
    in one change: `OxpilotSessionCreateRequest.target_id?`,
    `OxpilotSession.computer_use_target_id`,
    `OxpilotGrantListRequest.session_id?`, `OxpilotQueueDeleteRequest`
    + `OxpilotQueueDeleteResponse`, the `queueDelete` client method on
    `apiClient.oxpilot`. `make rpc-gen` then `make rpc-sync-check`.

- [ ] **Group 2 — Lib layer (`apps/web/src/lib/`).**
  - `lib/oxpilot-stream.ts` — `OxpilotFrame` gains the `tool_call`
    variant (the wire discriminant is `type:"tool_call"` VERBATIM —
    a `type:"tool"` arm would silently drop every tool frame; carries
    `call_id`/`call_index`/`tool`/`status`/`args_summary?`/`output_summary?`)
    and the `card` variant (`card_kind` + `ref_id`); the catch-all arm
    stays for forward tolerance. `OxpilotMessage` gains
    `tool_calls?: OxpilotToolCallCard[]` (the WF6
    `snapshot.messages[].tool_calls` merge and the Q5-A reseed need it).
    `openOxpilotStream(sessionId, onFrame, { onFatal })` — the new
    options param installs `es.onerror` with the `es.readyState`
    discriminator: `CONNECTING` ⇒ return (native retry already in
    flight — zero procs spent); `CLOSED` ⇒ `es.close()` + exactly ONE
    try/catch'd `apiClient.oxpilot.sessionGet{session_id}` →
    `onFatal(code)` (`not_found`, `oxpilot.disabled`, another code, or
    a probe throw — every arm terminates on the typed surface, never a
    dead stream). `postOxpilotChat`/`fetchOxpilotSnapshot` unchanged
    (the unit-test seam; the production send path is the
    `useOxpilotChat` fetcher).
  - `lib/oxpilot-queries.ts` — new queryOptions per the functional
    spec: `sessionGet`, `targetList` (enabled-gated), `grantList`
    (`{target_id?}` / `{session_id?}` filter shapes), `grantGet`,
    `queueList`, `diffList`, `diffGet`, `usageGet` (generated seam,
    unread in v1); new mutation helpers `pairingCreate`, `grantRespond`,
    `targetDisconnect`, `targetRevoke`, `sessionDelete`, `diffApply`,
    `messageQueue`, `queueDelete` (amendment #3), `turnCancel` (the
    optional `[Stop]` affordance). `retry:false` + `staleTime` 10–15 s
    everywhere; `invalidateOxpilotQueries` keeps batching the
    `["oxpilot"]` prefix.
  - `lib/oxpilot-chat.tsrx` — unchanged (fetcher + `toUiMessage` stay
    verbatim per the spec).

- [ ] **Group 3 — Components (`apps/web/src/components/oxpilot/` —
  14 files*, kebab-case `.tsrx`; Octane `function X() @{ … }` +
  `@if`/`@for`/`@switch`, never React JSX patterns).**
  - `flag-off.tsrx`* — `OxpilotFlagOff` lifted verbatim from the route
    (`flagOffReason` map, both env names, `docs/CONFIGURATION.md` link —
    ZERO interactive controls, SEC-9).
  - `session-row.tsrx`* — `OxpilotSessionRow`: title `AppLink`,
    `owner/repo · branch`, status badge text (never color-only),
    `formatRelative`, `focus-within` `[Delete]` → confirm.
  - `target-row.tsrx`* — `OxpilotTargetRow` + Manage expander:
    `label`, caps chips, lifecycle→text status (`pending_consent` ⇒
    "needs consent" + `[Connect]` re-offer that opens the expander —
    `grantList{target_id}` pending connect grants with
    `[Allow]`/`[Deny]` → `grantRespond`, BR7.4), `[Disconnect]`/
    `[Revoke]` confirms; `connected·busy` client-derived.
  - `new-session-dialog.tsrx`* — `NewSessionDialog` (M3): the repo
    `Select` (`SelectRoot`/`Trigger`/`Portal` moves INTO the dialog
    from `routes/oxpilot.tsrx`), branch `Select` via `repo.refs`
    (defaults to `default_branch`), goal `Textarea` autofocus, optional
    target `RadioGroup` (`None` default + `connected`-lifecycle targets
    only — a non-connected pick is refused client-side; `[Connect
    new…]` shortcut into `ConnectDialog`); `sessionCreate{repo_owner,
    repo_name, branch?, goal?, target_id?}` → invalidate + navigate;
    errors inline above actions; Esc/Cancel closes.
  - `connect-dialog.tsrx`* — `ConnectDialog` (M4 two-step): step 1
    `aria-label="Connect a system"` — `target_name` `Input`, caps
    `Checkbox`es (exec_shell + fs_read/fs_write pre-checked, GUI
    `disabled` + note), required understanding `Checkbox` gating
    `[Continue]` → `pairingCreate{target_name, caps}`; step 2
    `aria-label` re-announces "Pair the connector" — `CopyCommand`
    showing `ox pilot pair <code>` (the SOLE render site;
    `mutation.reset()` on close/step-2 exit), `targetList` poll every
    `PAIRING_POLL_MS` (2 s) matched on `pairing_id` bounded by
    `expires_at` → `grantList{target_id}` → `grantRespond{allow:true}`
    → connected + `[Done]`; timeout ⇒ error + retry; Esc/close
    abandons.
  - `confirm-dialog.tsrx`* — `OxpilotConfirmDialog`: the shared M5
    `AlertDialog` shell (aria-label names the object, Cancel
    `initialFocus`, danger styling on destructive confirms) — the ONLY
    `diffApply` path in the unit (SEC-12).
  - `sessions-rail.tsrx`* — `SessionsRail` (`nav` landmark): session
    list + active highlight, target status footer chips (`●N targets`
    + per-target name + status text — `connected·busy` derived from
    in-flight exec/fs tool cards, never polled), `[‹]/[›]` collapse
    `aria-expanded`, `[☰ Sessions]` disclosure <768px.
  - `transcript.tsrx`* — `OxpilotTranscript` (`main` landmark,
    `aria-live="polite"` `aria-atomic="false"`): boundary-only announce
    (`role="status"` announcer + `aria-hidden` stream tail — never per
    frame), autoscroll only while `atBottom` + "jump to latest" pill,
    closed card dispatch (`tool_call` `call_id`-keyed rows; `card_kind`
    → consent/diff/queued ref resolution; unknown kind ⇒ generic
    "update your client" notice card), assistant text plaintext
    `whitespace-pre-wrap` while `streaming` → `renderGfm` ONCE on
    `complete`.
  - `tool-call-card.tsrx`* — `OxpilotToolCall`: name +
    `args_summary`/`output_summary` as verbatim `<code>` + status text
    (running/done/failed — never markdown, never HTML).
  - `diff-card.tsrx`* — `OxpilotDiffCard`: `diffGet`-resolved
    `ProposedDiff` file stats, `[View diff]` → drawer, `[Show patch]`
    `<pre>` toggle, `[Create branch + PR]` (`pending` only) →
    `OxpilotConfirmDialog` → `diffApply` → `pr_ref` `AppLink` + toast;
    hosts dSIH sink #1 (the `contentHtml` highlighter output).
  - `consent-card.tsrx`* — `OxpilotConsentCard`: `grantGet`-resolved
    `GrantView` + target label; `role="group"` `aria-label="Action
    request"`; three verbatim buttons — `[Allow once]` →
    `{allow:true, scope:"once"}`, `[Allow for this session]` →
    `{scope:"session"}`, `[Deny]` → `{allow:false}` — never an
    "always" affordance; freeze + `grantGet` re-read on respond;
    `grant_not_pending`/`not_found` ⇒ refetch + freeze with the
    resolved outcome.
  - `queued-row.tsrx`* — `OxpilotQueuedRow`: `◐` + content +
    "queued — sends next" / "still queued — send after retry"; drops
    when `queueList` drains; the `[ix]`/M8 inline cancel wires
    `queueDelete` (ships iff amendment #3 lands).
  - `composer.tsrx`* — `OxpilotComposer` (`form` landmark): `draft`
    local state, send/queue branch (idle ⇒ `chat.sendMessage` POST
    fetch-reader; live turn ⇒ `messageQueue{session_id, content,
    client_msg_id}`), `crypto.randomUUID()` per send, the pinned
    `Ctrl/⌘+Enter` keydown + `aria-describedby` hint, per-code error
    banner slot; NEVER disables, `draft` clears only on successful send.
  - `context-drawer.tsrx`* — `ContextDrawer` (`complementary`, 320 px,
    overlay <1024 px): repo `owner/name · branch` via `sessionGet`,
    diff pane via `parseUnifiedDiffLines`/`highlightDiffLines`/
    `useLazyDiffRows` (dSIH sink #2), bound-target panel via
    `session_get.computer_use_target_id` → `targetList`; Esc closes +
    focus back to the toggle.

- [ ] **Group 4 — Routes (`apps/web/src/routes/`).**
  - `oxpilot.tsrx` — reworked to the M1 composition: `auth.me` gate +
    `usePageRedirect`, `oxpilot.status` probe → `OxpilotFlagOff` (M7) |
    `[+ New session]` `PageHeader` action → `NewSessionDialog` (the
    shipped inline `NewSessionCard` and its `SelectRoot`/`Trigger`/
    `Portal` move INTO the dialog — the route drops them), Sessions
    section (`h2`, `OxpilotSessionRow`s newest-first + `focus-within`
    delete → confirm; `EmptyState` + CTA on zero; inline notice +
    `[Retry]` on error), `Computer use` section (`h2 id="targets"`,
    `OxpilotTargetRow`s + empty-state + `[Connect a system]` →
    `ConnectDialog`); each section resolves
    `loading → empty | list | error(retry)` independently.
  - `oxpilot.$session.tsrx` — reworked to the three-pane
    `OxpilotChatSurface`: `Seed` extends to `{version, messages,
    streamingIds, activeTurn, pendingQueue, toolCalls, cards, session}`;
    `reseed()` runs the pinned 4-read set — `fetchOxpilotSnapshot` +
    `sessionGet` + `grantList{session_id}` + `diffList{session_id}`
    (parallel awaits folded into one `Seed`; drops to 3 reads iff the
    grant amendment is refused — recorded degraded path); the attach
    effect keeps the shipped core (first-snapshot skip via `attached`,
    later snapshots ⇒ `liveTail` clear + `onReseed`, `append` dedupe,
    `terminal` ⇒ invalidate + conditional reseed) and now feeds
    `tool_call`/`card` frames into the card dispatch + `onFatal` typed
    banners; keyed remount `s-${seed.version}`; rail + transcript +
    composer + drawer composition; auth gate + flag-off variant.
  - `settings/ai.tsrx` — gains an `AppLink` to `/oxpilot#targets` in
    the integrations surface (AC1.7.2).

- [ ] **Group 5 — Coverage manifests (`apps/web/src/test/`).**
  - `browser-coverage.manifest.ts` — rebinds the `oxpilot-repo-trigger`
    row's `surface:` from `routes/oxpilot.tsrx` to
    `components/oxpilot/new-session-dialog.tsrx` (the repo `Select`
    moved into the dialog; the route file would be undiscovered and
    orphan-fail); gains `oxpilot-new-session` (same surface — `Select` +
    `RadioGroup` inside `Dialog`) and `oxpilot-connect` (surface
    `components/oxpilot/connect-dialog.tsrx` — `Checkbox`/`Input` +
    step swap). These are the ONLY gate-discovered surfaces —
    `browser-coverage-check.ts:255-259` orphan-fails rows on
    undiscovered files; button-only surfaces (consent card, rail
    collapse, diff card cluster, session/target rows, drawer) ride the
    happy-dom route rows.
  - `route-coverage.manifest.ts` — UNTOUCHED: the two `oxpilot.*`
    happy-dom rows stay verbatim bound to `oxpilot.integration.test.ts`
    (the test file extends; the rows don't move).

- [ ] **Group 6 — Tests.**
  - `apps/web/src/lib/oxpilot-stream.unit.test.ts` — extended (shipped
    seam, not new): `tool_call`/`card` frame-variant parsing incl. the
    verbatim `type:"tool_call"` discriminant pin, `OxpilotMessage.tool_calls`
    merge, append-dedupe pass-through on non-append frames, and the
    `es.onerror` `readyState` probe split.
  - `apps/web/src/routes/oxpilot.integration.test.ts` — extended:
    targets section + Manage expander, both dialogs, all four card
    kinds, flag-off on both routes, the queue path, typed banners,
    mount-recovery reads, and the SEC-2/SEC-14 grep pins (enumerated in
    `unit-test-instructions.md`).
  - `apps/web/src/routes/oxpilot.browser.test.tsx` — extended per the
    manifest rows (the rebound repo-trigger subject +
    `oxpilot-new-session` + `oxpilot-connect`).
  - `crates/oxidean-db/tests/dialect_oxpilot.rs` — extended for the
    `computer_use_target_id` column round-trip + `SESSION_SELECT_*`
    parity ×3 dialects (flagged amendment #1 only).
  - `crates/oxidean-api/tests/oxpilot.rs` — extended for
    `session_create{target_id}`, `computer_use_target_id` on
    `session_get`/`session_list`, `grant_list{session_id}` narrowing,
    `queue_delete` ownership scoping (flagged amendments only).

- [ ] **Group 7 — Env/build + record.** `apps/web/package.json` +
  `bun.lock` untouched (TS-5 zero new deps); `make web-lint`,
  `make web-format-check`, `make route-coverage-check`,
  `make browser-coverage-check`, `make rpc-sync-check` green;
  `code-summary.md`, `source-manifest.json`, `traceability.json`
  (all US/AC/BR/NFR/SEC/REL ids → implementation or test file).

## Structural pins

The grep/assertion pins the design makes structural — they ride the
happy-dom integration file and the stream unit test (nfr-design Q7-A):

- **SEC-2 two-sink allowlist** — `dangerouslySetInnerHTML` under
  `components/oxpilot/**` + both oxpilot routes may appear ONLY inside
  `diff-card.tsrx`/`context-drawer.tsrx` (the shipped escape-built
  `contentHtml` diff-highlight sinks); `MarkdownHtml` is enforced at
  the component call site (its `dangerouslySetInnerHTML` lives in
  `components/repo/markdown-body.tsrx`, outside the pinned scope).
  ZERO `innerHTML`/`outerHTML`/`insertAdjacentHTML`/`document.write`/
  `createElement("script")`/`eval(`/`new Function(` in the same scope
  — asserted by a grep pin in `oxpilot.integration.test.ts` whose
  pattern is the API names (never a bare `script` substring).
- **`pairing_code` confinement** — `pairing_code`/`pairingCode`
  confined to `connect-dialog.tsrx` + the `CopyCommand` call site;
  never transcript subtree, live region, `console.*`, browser storage,
  or a second render site. `mutation.reset()` on dialog close/step-2
  exit drops the MutationCache copy (gc-bounded retention, never
  rendered).
- **`type:"tool_call"` verbatim discriminant** — the union arm matches
  the wire literal exactly; a `type:"tool"` arm never matches and would
  silently drop every tool frame — pinned in
  `oxpilot-stream.unit.test.ts` (REL-13).
- **`readyState` discriminator** — `es.onerror` spends ZERO procs on
  `CONNECTING` (native retry owns it) and exactly ONE try/catch'd
  `sessionGet` on `CLOSED` → `onFatal(code)`; never probe-per-drop,
  never blind retry, never a dead spinner (REL-3).
- **First-snapshot-skip** — the `attached` flag skips the first
  post-attach `snapshot` (the WF3 seed already rendered it); every
  LATER snapshot ⇒ wholesale replace (REL-1).
- **Keyed remount** — `setSeed(version+1)` remounts
  `OxpilotChatSurface` keyed `s-${seed.version}`; server-authoritative
  wholesale replace, never client-side merge.
- **Plaintext-stream → once-GFM** — assistant text renders
  `whitespace-pre-wrap` while `streaming`; `renderGfm` runs ONCE on
  `complete`; tool/card summaries stay verbatim `<code>`; diffs ride
  the escaped highlighter seams only (SEC-1/SEC-3/SEC-4).
- **Consent 3-action verbatim** — `[Allow once]`/`[Allow for this
  session]`/`[Deny]` fire literal `grantRespond` payloads
  (`scope:"once"`/`scope:"session"`/`allow:false`); no "always"
  affordance anywhere; `state!=="pending"` ⇒ buttons inert + frozen
  outcome line.
- **`mutation.reset()` on dialog close** — the pairing-code custody
  lifetime is dialog-step memory (SEC-14).
- **Zero UI telemetry** — no analytics, beacons, console metrics,
  error-reporting deps, `performance.mark`/`PerformanceObserver`
  (NFR6.1/OBS-3); console output is never the sole record of a failure.
- **One proc per mutation** — confirms fire a single
  `sessionDelete`/`targetDisconnect`/`targetRevoke`/`diffApply`;
  consent a single `grantRespond`; queue a single `messageQueue`;
  `invalidateOxpilotQueries` batches the refresh (NFR3.10/OBS-1).
- Supporting constants — dedupe key `"{message_id}:{seq}"`;
  `client_msg_id = crypto.randomUUID()` per send/queue; `staleTime`
  15 s status / 10 s lists, `retry:false` on auth-gated probes, ZERO
  `refetchInterval` anywhere (`PAIRING_POLL_MS` 2 s bounded by
  `expires_at` is the ONLY client poll — BR8.2).

## Notes

- Application code lands in `apps/web/` + `crates/` + `packages/` at
  workspace root — never under this record dir.
- The three flagged C1 amendments are additive, consumer-proposed,
  owner-committed deltas per the contract's deviation rule — this unit
  lands them because the design assigns it their ownership
  (`functional-spec.md` §"Contract deltas this unit carries"; Q2
  recommendation: land). Refusal degrades are recorded, never silent:
  no target column ⇒ the M3 radio + rail/drawer binding drop; refused
  `grant_list{session_id}` ⇒ the recovery read narrows to `diff_list`
  + the Manage-expander path (BR7.4); refused `queue_delete` ⇒ the
  "still queued — send after retry" line only.
- Measurable pins (NFR3 per-frame cost, NFR4 stream lifecycle, the
  SEC grep pins, the coverage rows, WCAG AA conformance) are inputs,
  not suggestions; surface gaps rather than weakening them.
