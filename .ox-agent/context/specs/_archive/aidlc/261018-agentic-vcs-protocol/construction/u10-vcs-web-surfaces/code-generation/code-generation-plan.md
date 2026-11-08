# Code Generation Plan — u10-vcs-web-surfaces

Upstream design stages (functional-design / nfr-requirements / nfr-design / infrastructure-design) were workflow-skipped for u7–u10 under the floor-16 forward jump; this plan embeds the design decisions those stages would have produced, sourced from the frozen contract (`inception/contract-design/contract-summary.md` C1/C4 + auth-scope note), `inception/units-generation/unit-of-work.md` §U10, and `inception/requirements-analysis/requirements.md` + `inception/user-stories/stories.md` (US1.1/US1.2/US2.7).

## Scope

Browser surfaces only — three existing web surfaces gain VCS awareness plus one new settings panel. No Rust, no wire changes, no API client regeneration (`vcs.*` methods already generated in `packages/api-client`; verify with `make rpc-sync-check`).

## Deliverables

1. [ ] **`apps/web/src/components/repo/vcs-settings-panel.tsrx`** — NEW `VcsSettingsPanel` (owns `VcsToggle`). Follows `lfs-settings-panel.tsrx`/`webhook-form.tsrx` conventions:
   - `<form method="post" action="#" onSubmit={preventDefault+save}>` wrapping a `<fieldset>`/`<legend>` titled **"Source control protocols"** (webhook-form fieldset precedent) containing the `Checkbox` component (`checked`, `disabled`, `aria-label`, `onCheckedChange` — Base UI keepMounted indicator already) + `<Label>` + description paragraph + `<Button type="submit">` "Save" — Enter submits the section's Save for free.
   - Status read: `useQuery` (`@octanejs/tanstack-query`) → `apiClient.vcs.status({owner, name})`. The `vcs.disabled` error envelope means never-enabled/disabled (AC1.2.3 — indistinguishable on the wire) → treated as `{enabled:false}` **unchecked-default**, NOT an error UI. Other failures throw → `role="alert"` load-error state. The query exposes `res.error.code` discrimination: on `!res.ok`, `code === "vcs.disabled"` returns the unchecked shape; anything else throws.
   - Draft state: checkbox drives a local `useState` draft seeded from query data; Save calls `apiClient.vcs.repoEnable({owner, name, enabled: draft})`; success → `toastSuccess` + `invalidateQueries` (status query key) + sync draft to response `enabled`; failure → inline `role="alert"` error (no toast for field errors — settings-page convention for section-level errors is inline `role="alert"`, mirror the LFS panel's error handling).
   - States (pinned): loading → "Loading VCS status…"; never-enabled → unchecked, saveable; enabled → checked; `archived` prop → checkbox + Save disabled + "Archived repositories are read-only." hint (server also denies writes — belt and suspenders per GIT-20 semantics); `can_admin` prop → when false render disabled control + "Only repository admins can change VCS settings." (dead state per UOW — the page gates `can_admin` first; kept for signature symmetry with sibling panels); saving → checkbox + Save inert, button copy "Saving…"; success → toast; error → inline alert.
   - Session cookies authorize `vcs.*` RPC (contract auth-scope note) — `apiClient` already sends credentials; no special auth handling.
2. [ ] **`apps/web/src/routes/$owner.$repo.settings.tsrx`** — mount `<VcsSettingsPanel owner={owner} name={repoName} can_admin={canAdmin} archived={!!repo.archived} />` in a `<div id="repo-settings-vcs" className="scroll-mt-28">` wrapper, ordered next to the other source-control-adjacent panels (after `repo-settings-lfs`, before `repo-settings-template`).
3. [ ] **`apps/web/src/components/repo/repo-settings-nav.tsrx`** — `REPO_SETTINGS_SECTIONS` gains `{ id: "repo-settings-vcs", label: "Source control" }` in the same position.
4. [ ] **`apps/web/src/components/repo/repo-activity-row.tsrx`** — `actionCopy` cases for the seven `vcs.*` kinds (C4 row shapes):
   - `vcs.repo_enable` → `updated the VCS protocol flag` (row carries no direction — `after_oid` null per C4)
   - `vcs.changeset_create` → `created a VCS changeset`
   - `vcs.merge` → `merged VCS changesets`
   - `vcs.rebase` → `rebased a VCS changeset`
   - `vcs.ref_update` → `created|deleted|updated VCS ref ${branch}` (before null → created; after null → deleted; else updated — `branch` var already prefers `ref_short`)
   - `vcs.git_import` → `imported Git history into VCS`
   - `vcs.git_export` → `exported VCS to Git ref ${branch}` (`ref_name` = `refs/heads/<b>`, `ref_short` supplies the short form when present)
   - Commit-link gate: `showCommitLink` additionally requires `item.after` to match a 40-hex git-oid shape (`/^[0-9a-f]{40}$/`). `cs_<ulid>` ids fail the shape → no `/commit/{sha}` href (contract C4 rendering note: `cs_` ids never route there — they 404); `vcs.git_export`'s real git oid passes → still links; existing push rows unchanged (their `after` is always 40-hex).
5. [ ] **`apps/web/src/routes/$owner.$repo.activity.tsrx`** — `TYPES` gains the seven `vcs.*` chips in a trailing group (order: `vcs.changeset_create` "VCS changesets", `vcs.merge` "VCS merges", `vcs.rebase` "VCS rebases", `vcs.ref_update` "VCS ref moves", `vcs.git_import` "VCS imports", `vcs.git_export` "VCS exports", `vcs.repo_enable` "VCS flag"). The `push_type` query filter already accepts `VCS_ACTIVITY_KINDS` (activity.rs:403-409) — exact-match per kind, no prefix grouping.
6. [ ] **`apps/web/src/components/repo/vcs-settings-panel.integration.test.ts`** — NEW happy-dom integration test (`renderWithQueryClient`, `vi.mock("@/lib/api-client")` + `@/lib/toast` per mirror-panel precedent): status→checked/unchecked states, `vcs.disabled`→unchecked-default, non-vcs error→alert, save flow calls `repoEnable`, save failure→inline alert, archived→disabled+hint, `can_admin:false`→disabled+admin copy.
7. [ ] **`apps/web/src/components/repo/vcs-settings-panel.browser.test.tsx`** — NEW Chromium test (`mountWithQueryClient`, `clickAriaLabel`, `expectNoOctaneOverlayInDocument`, `trackDomErrors` per repo-features-panel precedent): mount → toggle → Save → `repoEnable` mock called with `enabled:true`; failure path → alert rendered, no overlay.
8. [ ] **`apps/web/src/components/repo/repo-activity-row.integration.test.tsx`** — NEW happy-dom render test for `RepoActivityRow` with `vcs.*` items: `changeset_create` copy + `commit_message` rendered WITHOUT `/commit/` href (cs_ id); `git_export` copy + real 40-hex `after` DOES link to `commitHref`; `repo_enable`/`ref_update` copy; existing push row regression (git oid still links).
9. [ ] **`apps/web/src/test/browser-coverage.manifest.ts`** — manifest row: `{surface: "components/repo/vcs-settings-panel.tsrx", coverage: [{kind: "browser", test: <VCS_BROWSER>, subject: "vcs-settings-panel"}]}` — Checkbox is a high-risk surface; fresh skip-only rows are CI-rejected. Route-coverage manifest: no new rows (both routes already carry evidence rows).

## Pinned decisions (MUST implement)

- **`vcs.disabled` → unchecked-default, never error UI** (AC1.2.3 + UOW note): the status call failing with `vcs.disabled` is the never-enabled signal, not a failure worth alarming admins over. All OTHER query failures render the alert state.
- **Checkbox + explicit Save, not a Switch-immediate-commit**: the unit's pinned shape is "labelled checkbox inside fieldset/legend 'Source control protocols'" with a Save button; Enter inside the form submits Save (`method="post" action="#"` + `onSubmit` precedent).
- **`can_admin:false` is a documented dead state** (UOW): the settings page renders `RepoNotFound` for non-admins before panels mount; the panel still accepts the prop and renders the disabled+hint prong for signature symmetry — never dead code paths beyond that.
- **`archived` disables the control AND the Save button** with explanatory copy — server-side `ensure_not_archived` still denies the write; the UI gate is UX, not the enforcement boundary.
- **`cs_` changeset ids NEVER produce commit hrefs**: gate is the 40-hex git-oid shape on `after`, NOT the `vcs.*` push_type — `vcs.git_export` rows carry real git oids and MUST keep linking (the commit exists in the repo).
- **`vcs.ref_update` three-way copy** (create/delete/update) derives from `before_oid`/`after_oid` nullness — mirrors `branch_creation`/`branch_deletion` semantics.
- **Exact-match chips**: one chip per `vcs.*` kind (the API's `push_type` filter is exact-match against `VALID_PUSH_TYPES ∪ VCS_ACTIVITY_KINDS`); no prefix/grouped filter value exists — do NOT invent a `vcs` wildcard param.
- **Cookie auth rides the panel's RPC calls** (contract auth-scope note): `credentials:"include"` already configured on `apiClient`; the cookie denial applies only to `/api/vcs/*` object routes which the web app never calls.
- **No new credentials/endpoints**: the panel uses existing `apiClient.vcs.*` methods; object routes are for agents/CLI, not the browser.
- **No Git-behavior changes**: chips and copy additions only; existing `push`/`force_push`/`pr_merge`/branch rows render identically.
- **`commit_message` on VCS rows renders unlinked** when `after` is a `cs_` id (currently it would render a broken `/commit/cs_…` link — this is the bug this unit exists to prevent).
- **Commit shape**: `feat(web):` SemVer-prefixed subject for the production delta; artifact bookkeeping in a separate `docs(aidlc)` commit.

## Source manifest write-set

`apps/web/src/components/repo/vcs-settings-panel.tsrx`, `apps/web/src/routes/$owner.$repo.settings.tsrx`, `apps/web/src/components/repo/repo-settings-nav.tsrx`, `apps/web/src/components/repo/repo-activity-row.tsrx`, `apps/web/src/routes/$owner.$repo.activity.tsrx`, `apps/web/src/components/repo/vcs-settings-panel.integration.test.ts`, `apps/web/src/components/repo/vcs-settings-panel.browser.test.tsx`, `apps/web/src/components/repo/repo-activity-row.integration.test.tsx`, `apps/web/src/test/browser-coverage.manifest.ts`.

## Verification gates (order)

1. `bun run --filter @oxidean/web test:unit` (vitest unit + integration projects — happy-dom suite incl. new tests)
2. `make test-web-browser` (Chromium project — new browser test)
3. `make web-lint` (type-aware oxlint + octane DOM-race heuristic)
4. `make web-format-check` (oxfmt + prettier --check; run `bun run --filter @oxidean/web format` first)
5. `make browser-coverage-check-pr` (change-aware manifest gate vs origin/main)
6. `make rpc-sync-check` (expected no drift — wire types unchanged)
7. `cargo nextest run --workspace` (full regression — no Rust delta expected, still the checkpoint command)

CI legs gating this delta: `web-unit`/`web-integration`/`web-browser` shards, `browser-coverage`, `web-lint`, `format`, `rpc-sync-check`.

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "team",
  "ordering": "We implement the change first, then write and run the tests the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing expectations), and the full CI suite must be green before the PR can merge.",
  "scope": "classic",
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
      "Keep the existing test suite green.",
      "This scope adds no extra new-test floor beyond the selected test strategy."
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
  "input_sha256": "sha256:304bf27698e8b5f47dff4e16702de3e3c6f286e25e63e22facb420316bac75bc",
  "contract_sha256": "sha256:d7627c3e92082e54b594f33a1ba945e0e072964270fa08e0adcb8fe891ef16c6"
}
```

## Traceability

`traceability.json` maps each deliverable to contract sections (C4 activity-row/rendering notes + auth-scope note), unit-of-work §U10 lines, and requirement/story ids (FR1.1 flag toggle, FR1.4 disable-writes-keep-reads, FR16.1 activity rows, AC1.1.1 admin gate, AC1.2.1/AC1.2.3 disabled semantics, AC2.7.1–AC2.7.3 feed rendering/filterability).
