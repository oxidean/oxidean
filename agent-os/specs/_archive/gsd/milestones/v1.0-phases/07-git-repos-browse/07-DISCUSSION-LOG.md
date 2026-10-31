# Phase 7: Git Repos & Browse - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-12
**Phase:** 7-Git Repos & Browse
**Areas discussed:** Create flow, Browse IA & URLs, Visibility before orgs, Branches & archives, On-disk layout & gitoxide

---

## Create flow

| Topic | Options (summary) | Selected |
|-------|-------------------|----------|
| Entry | `/new` page vs modal vs you decide | ✓ `/new` page |
| Initial contents | empty / README / README+gitignore / you decide | ✓ Full templates (user directed) |
| Owner | user-only / dropdown for orgs / you decide | ✓ User-only |
| Slug rules | GitHub-ish / strict lower / you decide | ✓ GitHub-ish |
| Default visibility | public / private / you decide | ✓ Admin-configurable; unset → public |
| After create | always code / empty-state guide / you decide | ✓ Empty-state guide (forge-like) |
| Description | optional / skip / you decide | ✓ Optional |
| Template UX | grouped pickers / single preset / you decide | ✓ Grouped pickers |
| Default branch | always main / admin config / you decide | ✓ `main` + account settings override |
| Verify gate | block CTA+/new / form ok / you decide | ✓ CTA disabled + `/new` blocked |
| Duplicate name | inline / toast / you decide | ✓ Inline |
| Home | list / CTA only / you decide | ✓ Dashboard + list; activity placeholder |
| Empty home | hero / list empty / you decide | ✓ Hero |
| List order | recent / alpha / you decide | ✓ Recent |
| Visibility badge | yes / no / you decide | ✓ Yes |
| License catalog | curated / full SPDX / you decide | ✓ Full SPDX |
| gitignore catalog | broad / curated / you decide | ✓ Broad |
| Stack catalog | common / wide / you decide | ✓ Wide + community via in-repo+guide |
| Community presets | in-repo / operator dir / you decide | ✓ In-repo; marketplace later |

**Notes:** Activity feed deferred to later planning. Marketplace UI deferred but linked as future.

---

## Browse IA & URLs

| Topic | Options (summary) | Selected |
|-------|-------------------|----------|
| URL shape | `/{owner}/{repo}` / `/repos/...` / you decide | ✓ `/{owner}/{repo}` |
| Default Code | tree+README / tree only / you decide | ✓ Accept GH/Gitea tree+README |
| Tabs | four peers / Code primary / you decide | ✓ GitHub structure |
| Paths | tree/blob/raw / simpler / you decide | ✓ GitHub-like |
| Commit pages | yes / list only / you decide | ✓ Match GitHub |
| Blame | defer / include / you decide | ✓ Include |
| Highlighting | yes / plain / you decide | ✓ Yes; GH coverage + `.tsrx`/`.ripple` |
| Markdown | GFM / CommonMark / you decide | ✓ Safe GFM for parity |
| Line permalinks | yes / no / you decide | ✓ Yes |
| Binary | preview images / download only / you decide | ✓ Preview images |
| Ref in URL | branch name / SHA / you decide | ✓ Branch/tag names |
| Compare | defer / include / you decide | ✓ Include |
| File history | yes / no / you decide | ✓ Yes |
| Large files | soft / hard / you decide | ✓ Match GitHub soft limits |
| Submodules | entries / hide / you decide | ✓ Entries + recursive browse |
| Clone UI | box now / defer / you decide | ✓ Clone box now |

---

## Visibility before orgs

| Topic | Options (summary) | Selected |
|-------|-------------------|----------|
| Private meaning | owner-only / any signed-in / you decide | ✓ Owner-only |
| Anonymous public | yes / auth required / you decide | ✓ Yes |
| Denied access | 404 / 403 / you decide | ✓ 404 |
| Change visibility | settings toggle / fixed / you decide | ✓ Settings toggle |

---

## Branches & archives

| Topic | Options (summary) | Selected |
|-------|-------------------|----------|
| Branch CRUD who | owner / pushers / you decide | ✓ Owner |
| Default branch | block delete/rename / confirm allow / you decide | ✓ Block |
| Archive formats | zip+tar.gz / zip only / you decide | ✓ Both |
| Archive UI | Code menu / tags only / you decide | ✓ Code menu |

---

## On-disk layout & gitoxide

| Topic | Options (summary) | Selected |
|-------|-------------------|----------|
| Layout | bare `var/repos/...` / non-bare / you decide | ✓ Bare |
| Config | `OXIDEAN_REPOS_DIR` / hardcode / you decide | ✓ Env + volume |
| Backend | gix trait / gix only / you decide | ✓ **git CLI primary**; gitoxide later (user clarified) |
| Missing git | fail boot / soft-fail / you decide | ✓ Fail boot |
| Git version | 2.x / any / you decide | ✓ **2.5+** |
| Factory reset repos | wipe / keep / you decide | ✓ Admin chooses via **modal radios** |
| Repo delete | immediate disk / soft-delete / you decide | ✓ Soft-delete + async purge |
| Orphans | reconcile / ignore / you decide | ✓ Reconcile; admin sets frequency |
| GC | none / scheduled / you decide | ✓ Scheduled + admin trigger |
| Permissions | API user / you decide | ✓ API user |

**Notes:** User clarified CLI-vs-gix is backend-only; chose CLI because gitoxide lacks full 1:1 support yet. GIT-09 amended in CONTEXT.

---

## Claude's Discretion

- SPDX / gitignore packaging details
- Reserved-name denylist contents
- Reset modal option copy
- Soft-delete retention / purge timing
- Highlighting engine (must satisfy coverage)
- `GitBackend` trait shape

## Deferred Ideas

- Full home activity feed
- Org-level default branch settings (Phase 10)
- Public marketplace UI for stack presets
- HTTPS/SSH auth phases (8/9)
- Full branch protection product
- REQUIREMENTS.md GIT-09 wording update (follow-up docs)
