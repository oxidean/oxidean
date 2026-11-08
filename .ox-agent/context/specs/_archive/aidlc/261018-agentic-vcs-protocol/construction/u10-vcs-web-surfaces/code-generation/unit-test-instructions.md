# Unit Test Instructions — u10-vcs-web-surfaces

Web-side tests follow the repo's existing Vitest conventions: happy-dom integration (`*.integration.test.ts`) for Query/session UI via `renderWithQueryClient`, Chromium component tests (`*.browser.test.tsx`) for high-risk interactive surfaces via `mountWithQueryClient` + `expectNoOctaneOverlayInDocument` + `trackDomErrors`. Mock `@/lib/api-client` and `@/lib/toast` with `vi.mock` per `mirror-settings-panel.integration.test.ts` / `repo-features-panel.browser.test.tsx` precedent.

## Exact commands (runnable before first test step)

```bash
bun run --filter @oxidean/web test:unit -- vcs-settings-panel repo-activity-row   # new happy-dom suites
bun run --filter @oxidean/web test:unit                                           # full unit+integration regression
make test-web-browser                                                            # Chromium project (new browser test)
make browser-coverage-check-pr                                                   # manifest gate (vs origin/main)
make web-lint && make web-format-check                                           # type-aware oxlint + oxfmt/prettier
cargo nextest run --workspace                                                    # checkpoint command (no Rust delta)
```

## Coverage matrix — `VcsSettingsPanel` (`vcs-settings-panel.integration.test.ts`, happy-dom)

| Test | Asserts |
|------|---------|
| `status_enabled_checked` | `vcs.status` ok `enabled:true` ⇒ checkbox rendered checked; Save enabled |
| `status_disabled_unchecked_default` | `vcs.status` error `{code:"vcs.disabled"}` ⇒ checkbox UNCHECKED, no `role="alert"`, control saveable (AC1.2.3 never-enabled ≡ disabled) |
| `status_error_alert` | `vcs.status` error `{code:"repo.not_found"}` (or any non-`vcs.disabled`) ⇒ `role="alert"` with message, control NOT rendered |
| `save_enable_calls_repoEnable` | toggle + submit ⇒ `apiClient.vcs.repoEnable` called `{owner,name,enabled:true}`; success ⇒ `toastSuccess` + status query invalidated; draft synced |
| `save_error_inline` | `repoEnable` `!res.ok` ⇒ inline `role="alert"`, no toast, control remains interactive |
| `archived_disables` | `archived:true` ⇒ checkbox + Save disabled + "read-only" hint copy; submit path never calls `repoEnable` |
| `non_admin_dead_prong` | `can_admin:false` ⇒ checkbox + Save disabled + "Only repository admins" copy (dead state per UOW — page gates first) |
| `loading_state` | pending query ⇒ "Loading VCS status…" copy, no control |

## Coverage matrix — `VcsSettingsPanel` (`vcs-settings-panel.browser.test.tsx`, Chromium)

| Test | Asserts |
|------|---------|
| `toggle_save_flow_no_race` | mount ⇒ click checkbox (Base UI `data-slot="checkbox"`) ⇒ click Save ⇒ `repoEnable` called once ⇒ `expectNoOctaneOverlayInDocument` + `trackDomErrors` clean (the Checkbox indicator `keepMounted` race gate) |
| `save_failure_alert_no_overlay` | `repoEnable` rejects ⇒ `role="alert"` visible ⇒ no Octane overlay / DOM errors |

## Coverage matrix — `RepoActivityRow` (`repo-activity-row.integration.test.tsx`, happy-dom)

| Test | Asserts |
|------|---------|
| `vcs_changeset_create_copy_no_link` | `push_type:"vcs.changeset_create"`, `after:"cs_…"`, `commit_message` ⇒ "created a VCS changeset" copy; message rendered as plain `<span>` — NO `a[href*="/commit/"]` (cs_ id must not route to a git commit URL) |
| `vcs_git_export_links_git_oid` | `push_type:"vcs.git_export"`, `after:"<40-hex>"` ⇒ "exported VCS" copy + commit message DOES link to `commitHref(owner,repo,oid)` |
| `vcs_ref_update_variants` | `before:null` ⇒ "created VCS ref"; `after:null` ⇒ "deleted VCS ref"; both ⇒ "updated VCS ref" |
| `vcs_repo_enable_copy` | `push_type:"vcs.repo_enable"` ⇒ "updated the VCS protocol flag"; no commit link (null after) |
| `vcs_merge_rebase_import_copy` | `vcs.merge`/`vcs.rebase`/`vcs.git_import` ⇒ their copy strings |
| `push_regression_still_links` | git `push` row with 40-hex `after` + `commit_message` ⇒ commit link still rendered (no regression) |

## Coverage matrix — filter chips (`$owner.$repo.activity` integration or unit)

| Test | Asserts |
|------|---------|
| `vcs_chips_render` | TYPES block includes all seven `vcs.*` entries with `push_type`-value `value` fields |
| `vcs_chip_href` | each vcs chip's href carries `?type=vcs.<kind>` via `activityHrefWithFilters` |
| existing chips unchanged | git chips + period chips + active-class logic unchanged |

## Coverage matrix — settings mount (`$owner.$repo.settings` — covered by existing route suite + nav)

| Asserts |
|---------|
| `repo-settings-vcs` wrapper div mounts `VcsSettingsPanel`; `REPO_SETTINGS_SECTIONS` contains `{id:"repo-settings-vcs"}` between `repo-settings-lfs` and `repo-settings-template` — verified by the existing settings integration test's scroll/nav assertions plus the panel's own suite |
