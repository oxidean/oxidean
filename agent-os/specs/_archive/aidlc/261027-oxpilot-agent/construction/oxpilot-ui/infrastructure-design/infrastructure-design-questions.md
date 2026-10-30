# Infrastructure Design Questions — oxpilot-ui

Unit: `oxpilot-ui` (U8, kind `ui` — embedded `apps/web`) —
`OxpilotWeb` beyond the U1 shell: the M1 index, M3/M4 dialogs, the
M2 three-pane workspace, all in-transcript cards, the SSE client
extensions, and 14 new `components/oxpilot/*.tsrx` files plus
extensions to both shipped `/oxpilot*` routes and `lib/oxpilot-*`.
Infra concerns: which existing web CI lanes cover the new surfaces,
what deploy-time posture a static-bundle unit carries, and which
drift-gates the new feature dir trips.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U8 (kind
  `ui`, `embedded (apps/web)` — no new service by contract).
- [design] `construction/oxpilot-ui/{functional-design,
  nfr-requirements, nfr-design}/*` — the render/custody pins,
  the test trio, the coverage rows, the flagged C1 amendments.
- [contract] `contract-summary.md` (C1 consumed + flagged
  amendments; C2 frames) — what the generated client must carry.
- [sibling] `construction/skeleton-slice/infrastructure-design/*`
  — U1 already landed the `/oxpilot` routes, env block, and
  coverage-manifest rows this unit extends.
- [team] `team.md` — Testing Posture (web-octane vitest lanes,
  coverage manifests, weighted gate), Deployment (web image →
  Railway).
- [ci] `.github/workflows/ci.yml` (`web-octane` :299, `web-browser`
  :363, `route-coverage` :410, `browser-coverage` :442,
  `coverage-weighted` :474, `dead-code` :329, `commit-lint` :114,
  `rpc-sync` :639, `db-dialects` :855), `apps/web/Dockerfile`,
  `apps/web/vitest.config.ts`, `apps/web/src/test/{route,
  browser}-coverage.manifest.ts`, `Makefile` (`web-lint` :414-421,
  coverage targets :362-412), `docs/TESTING.md` (job table +
  ruleset rule :316-334).

## Q1. Does oxpilot-ui add deployable infrastructure?

- A. None — U8 lands entirely inside the existing `web` service:
  14 new `components/oxpilot/*.tsrx` files, extensions to the two
  shipped `/oxpilot*` routes (U1's skeleton), and `lib/oxpilot-*`
  extensions all compile into the same Astro static bundle that
  `apps/web/Dockerfile` builds and the `oxidean-web` Rust
  middleware serves (static assets + `/api/` proxy + SPA fallback
  — header comment, `apps/web/Dockerfile:1-3`). No new service,
  container, port, listener, or operator-provisioned resource —
  and no new route registration: `SHELLS`/`RESERVED_TOP_SEGMENTS`/
  routeTree already carry both oxpilot routes from U1.
- B. A separate oxpilot UI bundle/deployable — inverts the
  `embedded (apps/web)` unit contract; a second web artifact
  duplicates the image pipeline for zero isolation gain (the
  flag-off swap already severs the feature server-side).
- C. An edge/CDN layer for the stream attach — the SSE route is
  U1's api-side surface under `/api/`; the web tier already
  carries the `text/event-stream` compression exclusion per the
  shipped `stream_job_log` precedent (U1 infra spec).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — None — U8 lands entirely inside the existing `web` service. Conductor-decided under standing delegation; ratify at PR review.

## Q2. Which existing CI lanes must cover the new code — and do they?

- A. The shipped web lanes, zero edits: `web-octane` runs
  `make web-lint` (type-aware oxlint + `check-octane-dom-races` +
  `check-route-sync` + friends, `Makefile:414-421`), `format:check`
  (oxfmt + prettier), `bun run test` (vitest `unit` + `integration`
  projects — `oxpilot-stream.unit.test.ts` and the extended
  `oxpilot.integration.test.ts` replay here), the web build, and
  `make web-render-check`; `web-browser` replays the extended
  `oxpilot.browser.test.tsx` across the 5-shard Chromium legs;
  `route-coverage` keeps the two existing `/oxpilot*` manifest
  rows (`route-coverage.manifest.ts:789-807`); `browser-coverage`
  discovers the new high-risk `.tsrx` surfaces automatically and
  enforces the subject-keyed rows the manifest gains;
  `coverage-weighted` consumes the web unit/integration numbers
  via `make coverage-web`; `dead-code`'s knip pass covers the new
  `.tsrx` graph; `commit-lint` and `rpc-sync` inherit. Zero new
  jobs ⇒ no `Main` ruleset edit (`docs/TESTING.md:324`).
- B. A dedicated `oxpilot-ui` CI job — duplicates the workspace
  vitest projects and both coverage gates for zero new signal;
  the manifests already key rows to this unit's files.
- C. A stack-browser e2e lane for the full workspace — spec
  Scope rejects it (nfr-req Q7-B, TS-9): happy-dom + the shipped
  Chromium component tier cover the pinned surfaces; full
  pair→chat stack coverage is e2e debt, not a U8 gate.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — The shipped web lanes, zero edits: `web-octane` runs. Conductor-decided under standing delegation; ratify at PR review.

## Q3. Which drift-gates/docs surfaces does the feature dir trip?

- A. `browser-coverage.manifest.ts` — the gate's discovery scans
  every `apps/web/src/**/*.tsrx` for `Select*`/`Checkbox`/
  `RadioGroup`/`DialogPortal`/`AlertDialogPortal`/`Switch`/
  `DropdownMenu`/`form.Subscribe`
  (`docs/TESTING.md:198-205`), and the gate **orphan-fails**
  manifest rows whose surface file is undiscovered
  (`browser-coverage-check.ts:255-259`) — so only the two
  DISCOVERED surfaces carry browser rows
  (`oxpilot-new-session` + `oxpilot-repo-trigger` rebinding onto
  `components/oxpilot/new-session-dialog.tsrx` —
  SelectRoot/Trigger/Portal+RadioGroup; `oxpilot-connect` onto
  `components/oxpilot/connect-dialog.tsrx` — Checkbox); the
  remaining subjects (`oxpilot-session-row`, `oxpilot-target-row`,
  `oxpilot-consent`, `oxpilot-rail-toggle`, `oxpilot-diff-card`,
  `oxpilot-drawer` — button-only files, undiscovered) are
  exercised inside `oxpilot.integration.test.ts` under the two
  kept route rows (happy-dom has no subject level). The route
  manifest keeps its two rows verbatim (test extends, never swaps).
  `knip` covers the new dir (unused exports fail `dead-code`);
  `check-route-sync`/`web-render-check` inherit (no route or
  prerender additions). `rpc-sync` regenerates `api-client` for
  the flagged C1 amendment fields — `make rpc-gen` output is the
  only path (TS-6). `db-dialects` covers `0090` through the
  already-registered `dialect_oxpilot` binary + the filename-
  parity test — U1's `-E` term and TESTING.md row, unchanged.
- B. Manifest rows are optional documentation — the gates fail
  on discovered-without-row surfaces; a `.tsrx` using a Select or
  DialogPortal with no manifest entry is a CI-visible gap by
  construction.
- C. An OpenAPI/contract surface for the SSE routes — C2 is U1's
  shipped contract; the client extends the frame UNION
  (`tool_call`/`card` variants), not the routes; no drift-gated
  artifact exists for it.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `browser-coverage.manifest.ts` — the gate's discovery scans. Conductor-decided under standing delegation; ratify at PR review.

## Q4. Monitoring/telemetry infrastructure for the UI?

- A. None — OBS-3/NFR6.1 pin ZERO client-side telemetry: no
  analytics, beacons, console metrics, error-reporting dep, or
  `performance.mark`/`PerformanceObserver` instrumentation. The
  unit's diagnostic surface is UI-local typed banners + toasts +
  the flag-off `EmptyState` (nfr-design/observability-design.md
  OBS-4); the coverage manifests are the CI-visible behavior
  surface (OBS-5). Server-side stream/turn health is U3's
  `tracing` spans (agent-runtime monitoring-design — turn
  lifecycle/queue-drain/terminal signals) and U6's audit rows
  (connector-gateway monitoring-design — grant/target lifecycle);
  both are cited, not duplicated.
- B. A client error-reporting channel (Sentry-class) — the exact
  telemetry class NFR6.1 forbids, plus an egress path for
  transcript content the security design refuses.
- C. Client perf beacons for the typing-echo proxy — NFR3's proxy
  is guarded structurally (Q5-A at nfr-design: "an unshipped
  probe measures nothing"); happy-dom identity assertions are
  the CI leg.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — None — OBS-3/NFR6.1 pin ZERO client-side telemetry. Conductor-decided under standing delegation; ratify at PR review.

## Q5. Rollout/rollback posture for a static-bundle unit?

- A. The env flag IS the control: `OXIDEAN_OXPILOT_ENABLED`
  (default off — `.env.example:166`, `docs/CONFIGURATION.md:398`,
  `.railway/railway.ts:97-99` `preserve()`) severs every surface
  the UI renders — `oxpilot.status` answers `enabled===false`
  and the flag-off swap renders with ZERO controls (SEC-9);
  a mid-session proc-level `oxpilot.disabled` swaps identically
  (REL-14). A code rollback is the standard image repin — the
  bundle is stateless and the flagged `0090` column is additive
  (append-only migration, rollback-inert). The flagged
  amendments degrade on record: refused `grant_list{session_id}`
  narrows mount recovery to the Manage-expander path (NFR4.9);
  refused `session_create.target_id` drops the M3 picker —
  never silently.
- B. A per-route rollout flag client-side — adds a second
  behavior path the spec never pins; the server flag already
  gates every proc the UI can call.
- C. Bundle-splitting the feature for hot-disable — the two
  routes already code-split as shipped; a lazy boundary buys
  nothing the flag doesn't already sever, and the flag-off
  surface must render INSIDE the same bundle to explain itself.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — The env flag IS the control: `OXIDEAN_OXPILOT_ENABLED`. Conductor-decided under standing delegation; ratify at PR review.

## Q6. New dependencies or build posture changes?

- A. Zero new dependencies (TS-5/NFR3.11): every render need is
  already linked — `@tanstack/ai-octane ^0.9.1` +
  `@octanejs/tanstack-query ^0.1.67` (`apps/web/package.json:
  30,35`), the unified/remark/rehype sanitize toolchain behind
  `renderGfm` (`lib/markdown.ts`), `parse-unified-diff`/
  `highlight-diff`/`lazy-diff-rows`, `relative-time`, `toast`,
  and the `components/ui/*` primitive set plus the settings-surface
  `CopyCommand` (`components/settings/copy-command.tsrx`).
  `bun.lock` is untouched; CI's `bun install --frozen-lockfile`
  and the image's dep stage are unchanged; dependabot's weekly
  bun batch covers freshness.
- B. Vendor a stream-reconnect or SSE helper — native
  `EventSource` auto-reconnect IS the shipped mechanism (TS-3);
  a dep re-implements the platform for zero gain.
- C. A transcript-windowing lib for the 500-message proxy —
  recorded non-goal (NFR-S1): append-only render + the proxy
  bound stand until measurement says otherwise.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Zero new dependencies (TS-5/NFR3.11). Conductor-decided under standing delegation; ratify at PR review.

## Q7. What does the unit consume from the environment — and what must an operator provision?

- A. Nothing provisioned, nothing read: the UI holds no env
  access at all — it renders the NAMES `OXIDEAN_OXPILOT_ENABLED`/
  `OXIDEAN_OXPILOT_PROVIDER` as fix-naming copy (NFR8.1/NFR8.2)
  while the VALUES live server-side on U1/U4's documented chain;
  `oxpilot.status` is the read seam (gate-exempt, `retry:false`).
  `.env.example` and `docs/CONFIGURATION.md` are untouched by
  this unit; `.railway/railway.ts`'s three `preserve()` rows
  already cover enablement durability (U1). Per-user/per-repo
  provisioning: zero — repo binding is the session row, identity
  is the caller's session.
- B. A `PUBLIC_*`/build-time flag baked into the bundle — a
  build-time bake can't track the runtime flag and would lie on
  the flag-off surface; the status probe is the live answer.
- C. Operator-seeded feature records per user — enablement is
  instance-wide by design; the session row carries the binding.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Nothing provisioned, nothing read: the UI holds no env. Conductor-decided under standing delegation; ratify at PR review.

## Q8. Does the flagged `0090` migration change this unit's infra story?

- A. It is a cite, not a re-specification: amendment #1's
  additive `oxpilot_sessions.computer_use_target_id` column ships
  as `crates/oxidean-db/migrations/{mysql,postgres,sqlite}/
  0090_oxpilot_session_target.sql` in THIS unit's codegen (TS-10 —
  single-line ownership; U1 deferred it, U6's `0088` shipped
  without it). Its infra posture is the standard one: append-only
  ×3 dialects, applied by the existing boot runner, replayed by
  the `dialect_oxpilot` binary already registered in the
  `db-dialects` `-E` filterset (`ci.yml:902`) + the filename-
  parity test. It is an AMENDMENT — flagged, not yet shipped; a
  refused amendment drops the rail-chip/drawer target binding
  and the M3 picker with zero schema change.
- B. Re-specify the migration inside this stage — the db-layer
  contract (naming, ×3 parity, append-only policy) is
  `oxidean-db`'s owned surface; duplicating it here forks the
  rule.
- C. Fold the column into an existing migration — applied
  migrations are append-only; `0087`/`0088` are never edited.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — It is a cite, not a re-specification. Conductor-decided under standing delegation; ratify at PR review.

## Consolidated Summary Confirmation

- No deployable infra: the unit compiles into the existing
  `oxidean-web` static bundle — no service, port, or new route
  registration (Q1).
- CI: `web-octane` (lint/format/vitest trio/build/render-check),
  `web-browser` Chromium shards, `route-coverage` +
  `browser-coverage` gates, `coverage-weighted`, `dead-code`
  knip, `commit-lint`, `rpc-sync` — zero new jobs, no ruleset
  edit (Q2).
- Drift gates: subject-keyed browser rows on the two DISCOVERED
  surfaces only (`new-session-dialog` — incl. the `oxpilot-repo-trigger`
  surface rebind — and `connect-dialog`); undiscovered
  button-only subjects ride the kept happy-dom route rows; +
  knip coverage + rpc-gen client regeneration for flagged
  amendment fields + `db-dialects` coverage of `0090` via the
  existing `dialect_oxpilot` leg (Q3).
- Monitoring: zero client telemetry (OBS-3); UI-local typed
  banners/toasts are the surface; server-side stream/turn/grant
  health is U3's + U6's documented ownership (Q4).
- Rollout: `OXIDEAN_OXPILOT_ENABLED` severs every surface;
  flag-off swap is total; additive `0090` is rollback-inert;
  refused amendments degrade on record (Q5).
- Deps: zero new — `@tanstack/ai-octane`, query layer, GFM
  toolchain, diff libs, primitives all shipped; `bun.lock`
  untouched (Q6).
- Environment/provisioning: UI reads no env — renders env NAMES
  as copy while values live server-side; status probe is the
  seam; zero operator provisioning (Q7).
- `0090`: flagged amendment (not shipped), U8-owned, db-layer —
  standard append-only ×3 posture cited, not re-specified (Q8).

[Answer]: Looks correct
