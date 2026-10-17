# Workflow templates

The **New workflow** page (`/{owner}/{repo}/actions/new`) and `ox workflow template list` are backed by a catalog merged from three sources, lowest precedence first:

| Source | ID prefix | Where it lives |
|--------|-----------|----------------|
| Built-in | `builtin:` | Vendored `actions/starter-workflows` snapshot, embedded in the API binary |
| Instance | `instance:` | `instance_workflow_templates` table, managed under Admin → Templates |
| Org | `org:` | `<owner>/.github` repository, `workflow-templates/` directory |

`repo.actions.workflowTemplates.list` returns the merged set plus a `suggested` list computed from the repository tree; `repo.actions.workflowTemplates.get` returns one template's YAML with `$default-branch` substituted for the repo's actual default branch. `repo.actions.workflows.create` drafts a template (or arbitrary YAML) into `.github/workflows/` — see below.

## Built-in catalog

```text
crates/oxidean-api/assets/workflow-templates/
├── catalog.json        # upstream pin + file_patterns overlay
├── icons/              # *.svg served at /api/actions/template-icons/<name>.svg
├── ci/                 # <name>.yml + <name>.properties.json
├── deployments/
└── automation/
```

Each `<name>.yml` pairs with a `<name>.properties.json` (`name`, `description`, `iconName`, `categories`, `filePatterns`). **Upstream files are byte-identical to the pinned commit** — the checker enforces pairing, required fields, icon references, no stray files, and a 1 MiB total budget.

### Syncing

```bash
bun scripts/sync-workflow-templates.mjs --commit <sha>   # re-pin upstream + resync
bun scripts/sync-workflow-templates.mjs --check          # byte-for-byte drift check
make check-workflow-templates                            # structural validation
```

`catalog.json` records the pin under `upstream.repo` / `upstream.commit` / `upstream.synced`. Two directories are deliberately excluded (recorded in `catalog.excluded` with reasons): `code-scanning` (requires GitHub Advanced Security / CodeQL) and `pages` (GitHub Pages publishing).

### The `file_patterns` overlay

`catalog.json`'s `templates` map is an **Oxidean addition**, not upstream data. It maps template keys to extra `file_patterns` (e.g. `Cargo.toml` → `ci/rust`) used to compute the **Suggested** section by walking the repo tree. Upstream `.properties.json` `filePatterns` are also honored; the overlay extends them where upstream is thin.

## Instance templates

Admins manage instance templates under **Admin → Templates → Workflow templates** (procs `admin.workflowTemplates.*`, session-only): slug, name, description, icon, categories, file patterns, filename, YAML content, enabled flag. Enabled rows appear in every repo's catalog as `instance:<slug>`; YAML is validated at save time (`actions.invalid_workflow`).

## Org templates

A repo named `.github` under the same owner can carry `workflow-templates/<name>.yml` + an optional `<name>.properties.json` (GitHub's org convention). They appear as `org:<name>` in repos owned by that owner, with the `.github` repo as the source label.

## Runner-readiness hints

Two fields flag templates the bundled runner may not fully execute:

- `unsupported_actions` — `uses:` actions with no runner support (official templates are 1:1 GitHub starters, so `actions/setup-*` shows up here). The `ox workflow template list` RUNNER column shows `partial(<n> actions)`.
- `parse_error` — the workflow failed Oxidean's parser; it can still be committed, just not run. Shown as `unparsable`. Two vendored templates (`ci/generator-generic-ossf-slsa3-publish`, `ci/go-ossf-slsa3-publish`) currently hit this: they are **reusable-workflow jobs** (`jobs.<id>.uses:`) with no `runs-on`, which the parser requires.
