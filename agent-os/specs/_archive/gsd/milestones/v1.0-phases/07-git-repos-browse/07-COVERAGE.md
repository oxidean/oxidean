# API Coverage — system `git` CLI

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
> Phase 7 integrates the host/container `git` binary (≥2.5) behind the **`GitBackend`** identity noun (`CliGitBackend` shipped adapter; `GixGitBackend` future adapter) — D-32, D-33. Not a network SaaS API — same gate applies to external tool capability surfaces.

| capability | decision | reason |
|---|---|---|
| `git init --bare` | INTEGRATE | |
| `git symbolic-ref` (set unborn HEAD / default branch) | INTEGRATE | |
| `git rev-parse` / resolve refs | INTEGRATE | |
| `git show-ref` / list refs (branches + tags) | INTEGRATE | |
| `git ls-tree` | INTEGRATE | |
| `git cat-file` / `git show` blob content | INTEGRATE | |
| `git log` (paged) | INTEGRATE | |
| `git show` commit + patch | INTEGRATE | |
| `git diff` / compare two treeish | INTEGRATE | |
| `git blame` | INTEGRATE | |
| `git branch` create | INTEGRATE | |
| `git branch -m` rename | INTEGRATE | |
| `git branch -d`/`-D` delete | INTEGRATE | |
| `git archive` zip | INTEGRATE | |
| `git archive` tar.gz | INTEGRATE | |
| `git gc` | INTEGRATE | |
| `git --version` probe | INTEGRATE | |
| `git submodule` recursive network fetch | OPT-OUT | browse gitlink entries only (D-21); no network submodule clone in Phase 7 |
| `git worktree` / server-side checkout for browse | OPT-OUT | browse bare via plumbing; avoid working trees (RESEARCH anti-pattern) |
| smart HTTP `upload-pack` / `receive-pack` | OPT-OUT | Phase 8 HTTPS clone/push |
| SSH `git-upload-pack` / `git-receive-pack` | OPT-OUT | Phase 9 |
| `git lfs` | OPT-OUT | Phase 14 |
| `git tag` create annotated/lightweight | OPT-OUT | tags list + archive only in Phase 7 (UI-SPEC lock) |
| `git push` / `git fetch` as server actor | OPT-OUT | clients push in Phase 8/9; server does not push |
| `git merge` / `git rebase` on server | OPT-OUT | not a Phase 7 product surface |
| interactive `git` (pager/editor) | OPT-OUT | non-interactive argv only; never shell |
