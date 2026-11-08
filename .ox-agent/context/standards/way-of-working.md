# Way of working

- **Trunk-based development** on `main`. Every change reaches `main` through
  a pull request — the `Main` ruleset blocks direct pushes (sole bypass:
  the `oxidean-release` GitHub App for stable-cut commits).
- Branches are short-lived and named `type/<slug>` (`feat/`, `fix/`,
  `chore/`, `docs/`, `ci/`), often carrying the issue number
  (`fix/large-diff-render-110`). Parallel work runs in **git worktrees**
  under sibling checkouts.
- **Squash-merge** PRs: linear `main`, one commit per PR with the `(#NNN)`
  suffix; remote branches auto-delete after merge.
- Prefer the smallest change that fits an existing pattern and existing
  workspace deps over new frameworks or libraries.
- In-repo skills under `.agents/skills/` are the documented how-we-work —
  load a matching skill before inventing a workflow.
- Scratch/temp files go under gitignored `tmp/`, never the repo root or
  source trees.
- Deploy on merge to staging; production is a manual promote of a `main`
  ancestor with the latest CI green — see `docs/TESTING.md` and the
  `production-deploy.yml` workflow.
