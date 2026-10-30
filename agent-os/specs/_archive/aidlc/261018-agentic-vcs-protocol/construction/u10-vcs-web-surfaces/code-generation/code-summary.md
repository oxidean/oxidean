# Code Summary — u10-vcs-web-surfaces

## What landed

The two web surfaces for the VCS protocol: a repository-settings panel that enables/disables the protocol per repo, and activity-feed rendering + filter chips for the seven `vcs.*` activity kinds recorded by the U7 API surface. All domain calls go through `@oxidean/api-client` (`apiClient.vcs.status` / `apiClient.vcs.repoEnable`) — no fetch, no raw RPC.

### `components/repo/vcs-settings-panel.tsrx` (new)

Octane `VcsSettingsPanel` mounted under the repo settings nav. Behaviour per the U10 plan:

- `vcs.status` query loads current state; `vcs.disabled` maps to a normal unchecked draft (not an error); any other status error renders an alert state.
- Controlled `Checkbox` draft inside the existing `fieldset`/`legend` accessibility pattern; explicit `Save protocol setting` submit (`method="post" action="#"`, `onSubmit` preventDefault, `type="submit"` button — Enter-submits-Save for free).
- Save calls `apiClient.vcs.repoEnable({ owner, name, enabled })`; success invalidates/refetches status and toasts `"VCS protocol enabled."` / `"VCS protocol disabled."`; failure preserves the draft and renders the server message inline.
- Controls disable while loading/saving, when the repo is archived (with a visible read-only explanation), and when the caller lacks admin (visible non-admin explanation).
- Checkbox indicator uses `keepMounted` — the Octane DOM-race precedent for Base UI controls.

Mounted in `routes/$owner.$repo.settings.tsrx` behind the existing admin gate; `repo-settings-nav.tsrx` gains the matching section row.

### `components/repo/repo-activity-row.tsrx`

- `actionCopy` gains cases for all seven `vcs.*` kinds (`vcs.repo_enable`, `vcs.changeset_create`, `vcs.merge`, `vcs.rebase`, `vcs.ref_update`, `vcs.git_import`, `vcs.git_export`) with human copy.
- Commit-link gate now keys on a 40-hex OID check: `vcs.git_export` rows (real Git OIDs in `before`/`after`) still link to `/commit/{sha}`; `cs_<ULID>` changeset ids never link.

### `routes/$owner.$repo.activity.tsrx`

Seven new filter chips — one per `vcs.*` `push_type` — riding the existing `type` query-param handling (exact-match server-side; `repo.activity.list` already accepts `VCS_ACTIVITY_KINDS`).

### Coverage manifests

`browser-coverage.manifest.ts` gains `VCS_BROWSER` + a surface row for `components/repo/vcs-settings-panel.tsrx` (Checkbox = high-risk control → Chromium evidence required). Existing route-coverage rows for the settings/activity routes remain valid.

### Tests

- `vcs-settings-panel.integration.test.ts` (8 tests, happy-dom) — status enabled/disabled/error/loading states, save success (payload + toast + refetch), save failure (draft preserved + inline error), archived and non-admin disabled states. Base UI quirk handled: tests click the visually-hidden native `input[type=checkbox]` (label-forwarding target) and assert `aria-checked` on the `span[role=checkbox]` root.
- `vcs-settings-panel.browser.test.tsx` (2 tests, real Chromium) — Checkbox toggle + form submit calls `repoEnable` with `expectNoDomRaces()`; save-failure renders inline alert without Octane overlay. `act` imported from `octane` (the testing-library path pulls `aria-query`, whose ESM build lacks `elementRoles` under Chromium).
- `repo-activity-row.integration.test.tsx` (6 tests) — copy for every `vcs.*` kind; `cs_` ids render unlinked; real Git OIDs (git_export) still link.

## Gate results

| Gate | Result |
|------|--------|
| `bun run --filter @oxidean/web lint` | clean |
| `bun run --filter @oxidean/web format:check` | clean |
| `scripts/check-octane-dom-races.ts` | OK |
| `scripts/check-internal-anchors.ts` | OK |
| `scripts/check-route-sync.ts` | 78 shells ↔ 79 pages |
| `bun run --filter @oxidean/web test` (unit+integration) | **609/609** |
| `bun run --filter @oxidean/web test:browser` (panel file) | **2/2** Chromium |
| `make browser-coverage-check-pr` | PASS (7 touched / 51 discovered) |
| `make rpc-sync-check` | ok (generated client drift-free) |
| `cargo nextest run --workspace` | **1478/1478** |
