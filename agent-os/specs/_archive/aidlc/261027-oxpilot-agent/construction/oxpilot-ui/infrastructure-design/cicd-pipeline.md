# CI/CD Pipeline — oxpilot-ui (U8)

Consumes: `infrastructure-design-questions.md` (Q2/Q3/Q5/Q6/Q8),
`team.md` Testing Posture + Deployment, `.github/workflows/ci.yml`,
`apps/web/vitest.config.ts`, `apps/web/src/test/{route,browser}-
coverage.manifest.ts`, `Makefile` (`web-lint` :414-421, coverage
targets :362-412), `docs/TESTING.md` (job table + ruleset rule
:316-334), `nfr-design/*` (Q7-A manifest pins, SEC-2/SEC-14 grep
pins).

**No new jobs, no ruleset edit.** Every surface lands inside the
existing web lanes — the work is landing the code where the
filters already see it and registering the rows the discovery
scans demand. Per `docs/TESTING.md:324`, adding/renaming/removing
a job or matrix value requires a `Main` ruleset update in the
same change — **explicitly out of scope**: nothing here adds,
renames, or removes a CI job.

## Lane coverage

| lane | what covers U8 | evidence it fires |
|------|----------------|-------------------|
| `ci.yml` `web-octane` (:299) | `make web-lint` — type-aware `oxlint` + `check-octane-dom-races` + `check-internal-anchors` + `check-template-semicolons` + `check-route-sync` + `check-pending-before-notfound` (`Makefile:414-421`) lints every new `.tsrx`; `format:check` (oxfmt + prettier); `bun run test` = vitest `unit` + `integration` projects (`vitest.config.ts:123-147`) — `lib/oxpilot-stream.unit.test.ts` (frame-union variants, append dedupe, malformed skip, the `es.onerror` `readyState` probe split, the verbatim `type:"tool_call"` discriminant pin) + the extended `routes/oxpilot.integration.test.ts` (happy-dom: targets section, both dialogs, four card kinds, flag-off, queue path, the SEC-2 two-sink/`innerHTML`-class grep pins + `pairing_code` confinement pin) replay here; `bunx turbo run build --filter=@oxidean/web` compiles the bundle; `make web-render-check` post-build scans (issues #165/#169) inherit | `ci.yml:313-324` step sequence; `apps/web/package.json` `lint`/`test`/`format:check` scripts |
| `ci.yml` `web-browser` (:363) | the extended `routes/oxpilot.browser.test.tsx` — subject-keyed mounts for the two discovered surfaces (`oxpilot-repo-trigger` + `oxpilot-new-session` on `new-session-dialog.tsrx`, `oxpilot-connect` on `connect-dialog.tsrx`) — replays across the 5-shard Chromium legs | `ci.yml:384-405` shard loop + `VITEST_BROWSER_CONNECTION_CLOSED` retry arm |
| `ci.yml` `route-coverage` (:410) | `make route-coverage-check` → `scripts/route-coverage-check.sh`: the two existing `/oxpilot*` rows STAY verbatim — `route-coverage.manifest.ts:789-807` already binds both routes to `oxpilot.integration.test.ts` (happy-dom); the test extends, the rows don't move | manifest rows :790-807; change-aware mode is satisfied — the rows are `happy-dom`, not skip-only |
| `ci.yml` `browser-coverage` (:442) | `make browser-coverage-check` + `scripts/ui-coverage-change.contract.sh`: the gate's discovery scans every `apps/web/src/**/*.tsrx` for `Select*`/`Checkbox`/`RadioGroup`/`DialogPortal`/`AlertDialogPortal`/`Switch`/`DropdownMenu`/`form.Subscribe` (`docs/TESTING.md:198-205`) and **orphan-fails manifest rows whose surface file is not discovered** (`scripts/browser-coverage-check.ts:255-259`) — only `new-session-dialog.tsrx` (`SelectRoot`/`SelectTrigger`/`SelectPortal`/`RadioGroup`) and `connect-dialog.tsrx` (`Checkbox`) are discovered among the new files, so only those surfaces carry browser rows (below); button-only surfaces stay on the happy-dom route rows | `ci.yml:458-469`; `browser-coverage.manifest.ts:562-572` keeps `oxpilot-repo-trigger` (surface rebinds — below) |
| `ci.yml` `coverage-weighted` (:474) | `make coverage-contract` (`Makefile:394-395`) + `make coverage-web` (`Makefile:362-372`) collects web unit/integration `coverage-summary.json` for the extended tests; the 0.70 weighted floor and the frozen e2e checklist are untouched | `ci.yml:505-540` |
| `ci.yml` `dead-code` (:329) | `bunx knip` covers the new `.tsrx`/`.ts` graph (custom compilers in `knip.ts`) — unused exports/dead files in `components/oxpilot/` fail the job | `ci.yml:349-350` |
| `ci.yml` `rpc-sync` (:639) | `make rpc-sync-check` → `scripts/check-rpc-sync.sh`: the flagged C1 amendment fields (`session_create.target_id`, `computer_use_target_id` on session views, `grant_list.session_id`, `queue_delete`) regenerate `packages/api-client` via `make rpc-gen` — Rust is the source of truth; hand-edited client is never it (TS-6). **Amendments are flagged, not shipped** — the regen lands only if the amendments are accepted | `ci.yml:666-667` |
| `ci.yml` `db-dialects` (:855) | `0090_oxpilot_session_target.sql` ×3 dialects (flagged amendment #1 — U8-owned, db-layer): replays through the `dialect_oxpilot` binary already in the `-E` filterset (`ci.yml:902`) + the migration filename-parity test enforcing ×3 presence — U1's leg and TESTING.md row, unchanged | `ci.yml:902` filterset; `docs/TESTING.md` `db-dialects` row |
| `ci.yml` `rust-build`/`api-rust` shards | the workspace nextest archive compiles any Rust-side change the `0090` column touches (`oxidean-db` write/read surface for `computer_use_target_id` — amendment-contingent); archive is workspace-wide, no `-E` term needed | `ci.yml` nextest-archive step |
| `ci.yml` `commit-lint` (:114) | PR title + range ride the SemVer grammar; the unit lands under `feat(web)`/`feat(oxidean-web)` scope | `ci.yml:114-115` |

## Coverage manifest rows the design pins (nfr-design Q7-A)

| manifest | rows | status |
|----------|------|--------|
| `route-coverage.manifest.ts` | `oxpilot.tsrx` + `oxpilot.$session.tsrx` → `happy-dom` on `oxpilot.integration.test.ts` (:790-807) | **kept verbatim** — U1's rows; the test file extends |
| `browser-coverage.manifest.ts` | `oxpilot-repo-trigger` (:564-570) | **kept, surface rebinds** — `surface:` moves `routes/oxpilot.tsrx` → `components/oxpilot/new-session-dialog.tsrx` (the `Select` moves into the dialog; the route file would be undiscovered and orphan-fail) — the repo `Select` portal race gate |
| `browser-coverage.manifest.ts` | `oxpilot-new-session` → `browser` on `oxpilot.browser.test.tsx`, surface `components/oxpilot/new-session-dialog.tsrx` | **gained** — `Select`+`RadioGroup` inside `Dialog` (discovered file) |
| `browser-coverage.manifest.ts` | `oxpilot-connect` → `browser` on `oxpilot.browser.test.tsx`, surface `components/oxpilot/connect-dialog.tsrx` | **gained** — two-step `Checkbox`/`Input` + step swap (discovered file) |
| `route-coverage.manifest.ts` | subjects `oxpilot-session-row`, `oxpilot-target-row`, `oxpilot-consent`, `oxpilot-rail-toggle`, `oxpilot-diff-card`, `oxpilot-drawer` | **no new rows** — route-coverage binds route→test-file only (no subject level); these surfaces are exercised inside `oxpilot.integration.test.ts` under the two kept route rows — consent card buttons, rail collapse, diff card cluster + drawer carry no discovery primitive, and pinning them as browser rows would orphan-fail the gate (`browser-coverage-check.ts:255-259`) |

## Supply chain

- Zero new dependencies (TS-5/NFR3.11): `@tanstack/ai-octane
  ^0.9.1`, `@octanejs/tanstack-query ^0.1.67`, the unified/
  remark/rehype GFM toolchain, diff/highlight/lazy-rows libs,
  `relative-time`, `toast` are all shipped
  (`apps/web/package.json`); `bun.lock` untouched — CI's
  `bun install --frozen-lockfile` and the Dockerfile dep stage
  are unchanged; dependabot's weekly bun batch covers freshness.
- Test infra is the shipped trio (TS-9): happy-dom +
  `@octanejs/testing-library` + the Playwright Chromium project —
  no new harness, no stack-browser e2e (spec Scope).

## Deployment + rollback

Unchanged rolling path (PR → `pr-<n>` → ephemeral env → squash →
`sha-<sha>` → staging → manual promote); the bundle rides the
`web` image — `images.yml` untouched. Rollback is the standard
image repin + the flag: `OXIDEAN_OXPILOT_ENABLED` default-off
means a green deploy ships dark, and disabling severs every
surface with zero deploy (flag-off swap is total — SEC-9). The
flagged `0090` column is additive/append-only: a code rollback
leaves a harmless unused column; a refused amendment leaves no
column at all.

## Explicitly NOT in this unit

- No new CI job and no `Main` ruleset required-status-checks
  edit — `docs/TESTING.md:324` makes job-shape changes a
  same-change ruleset obligation; this unit changes none.
- No stack-browser/e2e-stack lane — spec Scope rejects it; the
  shipped happy-dom + Chromium component tiers cover the pinned
  surfaces (TS-9; recorded e2e debt, not a gate).
- No env/`railway.ts` edit — the three `preserve()` rows are
  U1's; the UI reads no env.
- No `.env.example`/`docs/CONFIGURATION.md` edit — the env block
  is U1's documented surface; the flag-off copy cites it by
  name only.
- No `cli-parity`/SKILL surface — the dialog renders the
  `ox pilot pair <code>` STRING (U7's verb as copy); no `ox`
  command surface changes here.
- No route registration — `SHELLS`/`RESERVED_TOP_SEGMENTS`/
  routeTree/`check-route-sync` are untouched; both routes are
  U1's.
- No coverage-floor or checklist change — the weighted gate and
  the frozen e2e checklist are the affirmed posture.
