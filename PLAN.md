# Plan: Migrate AI-DLC + archived GSD → Agent OS (uniform spec-driven setup)

> One-paragraph summary: Replace the AI-DLC framework (and the read-only GSD
> archive) with the Builder Methods stack — **Agent OS** as the system of
> record for standards, product docs, and specs; **PRD Creator** for
> idea→PRD→milestone generation; **agentcanon** as the canonical-file
> convention (`.agents/skills/` + `AGENTS.md` real, everything else symlinks).
> All historical spec material (AI-DLC intents, GSD milestone docs) is folded
> into `agent-os/specs/` so nothing is lost, and every harness reads the same
> files.
>
> **Guiding principle — one place of truth (agentcanon):** every kind of
> artifact has exactly one canonical home; everything else is a symlink or a
> short pointer file, never a second copy. Skills live only in
> `.agents/skills/`, instructions only in `AGENTS.md`, specs only in
> `agent-os/specs/`, product docs only in `agent-os/product/`, standards only
> in `agent-os/standards/`. No `_build_plan/`, no `.claude/commands/` mirror,
> no parallel skill directories, no `docs/archive/` holdouts.

## Current state (what exists today)

### AI-DLC footprint (to be removed after content is migrated)

| Location | Contents |
|----------|----------|
| `aidlc/` (39 MB) | `spaces/default/{memory,knowledge,codekb,intents}` + untracked `active-memory/` mirror + machine-local runtime (`.aidlc-*`, sessions) |
| `.claude/` (8.2 MB) | Almost entirely AI-DLC: `CLAUDE.md`, `agents/` (14 `aidlc-*` personas), `aidlc-common/`, `hooks/` + wiring, `knowledge/`, `rules/aidlc.md`, `scopes/`, `sensors/`, `settings.json` (aidlc perms/hooks/statusLine), `settings.local.json.example`, `tools/` (~80 `aidlc-*.ts` engine sources), `skills/` (42 real `aidlc-*` dirs + 2 symlinks) |
| `.cursor/` (8.4 MB) | Same AI-DLC surface: `agents/`, `aidlc-common/`, `cli.json` (aidlc-only perm), `hooks/` + `hooks.json` (all aidlc engine calls), `knowledge/`, `scopes/`, `sensors/`, `skills/` (47 aidlc dirs), `tools/` + `data/stage-graph.json`, and 6 aidlc rules in `rules/` |
| `.agents/skills/` | `aidlc*` entries are **symlinks into `.cursor/skills/`**; the real project skills (octane, ox, tdd, rust-*, semver-conventions, …) live here already |
| `AGENTS.md` | "Planning / AI-DLC" section (~L99–102) + `BEGIN/END AI-DLC:agents` block (~L114–174) |
| `ROADMAP.md` | 1 line: "executable planning runs through AI-DLC (`/aidlc`; workspace under `aidlc/`)" |
| `docs/CODE_PRACTICES.md` | 1 line referencing `aidlc/` + `docs/archive/planning/` |
| `.gitignore` | AI-DLC block (~L60–118) |
| Global | `~/.local/bin/aidlc` binary (`aidlc 2.11.1`) + scattered global skill dirs (`~/.claude/skills`, `~/.codeium/windsurf/skills`, `~/.config/devin/skills`, `~/.copilot/skills`) — handled in Phase 5 |

### Non-AI-DLC files that must survive inside `.claude/` and `.cursor/`

- `.cursor/rules/`: `oxidean-core.mdc`, `octane-ui.mdc`, `rpc-codegen.mdc`,
  `rust-crates.mdc`, `use-in-repo-skills.mdc`, `clean-user-facing-text.mdc`,
  `no-ai-attribution.mdc` — keep.
- `.claude/settings.local.json` — gitignored user file, leave alone.
- After cleanup, `.cursor/` reduces to `rules/` only and `.claude/` reduces to
  the agentcanon symlink + local files.

### Spec material to preserve ("merge into agent-os")

1. **AI-DLC intents** under `aidlc/spaces/default/intents/`:
   - `261014-semver-rollout` — completed; full phase/stage artifact tree +
     audit shards committed.
   - `261018-agentic-vcs-protocol` — completed; same structure.
   - `261027-oxpilot-agent` — **in flight**. Its committed artifacts
     (`aidlc-state.md`, `project-description.json`, `ideation/`, `inception/`,
     `construction/`, `verification/`, `audit/`) live on branch
     `feat/oxpilot-agent` (draft PR #203). On this branch the record dir
     holds only gitignored runtime state.
2. **AI-DLC memory/method**: `aidlc/spaces/default/memory/` (and its
   `active-memory/` copy): `org.md`, `team.md`, `project.md`,
   `phases/{ideation,inception,construction,operation}.md`. Contains real,
   affirmed practices — trunk-based dev, worktree usage, squash-merge,
   SemVer commit grammar, guard policy `relaxed`, testing posture learnings.
3. **AI-DLC knowledge**: `aidlc/spaces/default/knowledge/` (user docs under
   `knowledge/documents/`; derived `documentkb/` catalog).
4. **GSD archive**: `docs/archive/planning/` (6.2 MB, read-only) — `STATE.md`,
   `PROJECT.md`, `REQUIREMENTS.md`, `MILESTONES.md`, `ROADMAP.md`,
   `milestones/v1.0-*` (per-phase `PLAN`/`SUMMARY`/`CONTEXT`/`RESEARCH`/
   `VALIDATION`/`VERIFICATION` files), `todos/`, `parallel-tracks/`,
   `research/`.
5. **Product/direction sources outside both systems**: `README.md`,
   `ROADMAP.md` (product backlog), `docs/ARCHITECTURE.md`,
   `docs/VERSIONING.md`, `docs/TESTING.md`, `docs/CODE_PRACTICES.md`,
   `AGENTS.md` stack table, `.cursor/rules/*.mdc` (project rules).

## Target layout

```
agent-os/
  product/
    mission.md            # synthesized: AGENTS.md "Product" + GSD PROJECT.md
    roadmap.md            # ROADMAP.md content moves here (repo root keeps a pointer)
    tech-stack.md         # AGENTS.md stack table + ARCHITECTURE.md summary
    decisions.md          # "Decided" entries from memory team.md/project.md + learnings
    prd.md                # produced by bm-prd-creator when needed (not on day one)
    milestones/           # bm-prd-creator milestone prompts (when used)
    milestone-log.md      # bm-prd-creator log (when used)
  standards/
    index.yml             # maintained by index-standards / discover-standards
    way-of-working.md     # from team.md: trunk-based dev, worktrees, squash-merge, branches
    commits-versioning.md # SemVer commit grammar, versioning.toml, release flow
    code-style.md         # project.md + CODE_PRACTICES.md distilled
    testing.md            # testing posture + learnings (docker/act, gates)
    review-practices.md   # what guard-policy/sensors used to enforce, as plain rules
    web-ui.md             # Octane rules (pointer to .cursor/rules/octane-ui.mdc + skill)
    rust.md               # rust-crates rules pointer
  specs/
    _archive/
      gsd/v1.0/                    # docs/archive/planning/ moved wholesale
      aidlc/memory/                # org/team/project/phases method files (raw)
      aidlc/261014-semver-rollout/
      aidlc/261018-agentic-vcs-protocol/
      aidlc/261027-oxpilot-agent/  # once merged (see sequencing)
    <YYYY-MM-DD-HHMM-slug>/        # new specs from shape-spec
      plan.md / shape.md / standards.md / references.md / visuals/
.agents/skills/                    # agentcanon canon — real skill folders
  agent-os-discover-standards/
  agent-os-index-standards/
  agent-os-inject-standards/
  agent-os-plan-product/
  agent-os-shape-spec/
  bm-prd-creator/
  ...existing project skills (unchanged)...
.claude/skills -> ../.agents/skills    # agentcanon symlink (replaces 44 entries)
CLAUDE.md -> AGENTS.md                 # agentcanon symlink at repo root
AGENTS.md                              # real instructions; AI-DLC block → Agent OS section
```

Notes on the mapping:

- **agent-os commands → skills (decided)**: upstream installs commands as
  `.claude/commands/agent-os/*.md`, which only Claude Code reads — that would
  be a second, harness-specific surface. Instead, each of the 5 command files
  is vendored as a thin `.agents/skills/agent-os-<name>/SKILL.md` (frontmatter
  + body copied verbatim), the repo's existing vendoring convention.
  `.claude/commands/agent-os/` is **not** created — skills are the single
  invocation surface on every harness. Recorded in `skills-lock.json` like
  the other vendored skills.
- **PRD Creator output**: upstream writes `_build_plan/`. The vendored copy is
  patched to write into `agent-os/product/` (`prd.md`, `milestones/`,
  `milestone-log.md`) so *all* spec/planning artifacts live under `agent-os/`.
  Pipeline: `bm-prd-creator` (idea → PRD + milestone prompts in product/) →
  `agent-os-shape-spec` per milestone/feature → `agent-os/specs/<spec>/`.
- **GSD + AI-DLC archives**: moved, not copied — `docs/archive/planning/` and
  `aidlc/` go away; content survives under `agent-os/specs/_archive/` keeping
  its internal structure (it's already organized per-unit/per-phase).
  `.aidlc-engine/`, `.aidlc-construction-checkpoints/`, `runtime-graph.json`,
  `intents.json`, cursor files, and the `active-memory/` mirror are dropped
  (runtime/derived state). `audit/` shards are kept — they're the committed
  decision record.
- **knowledge/**: `knowledge/documents/` (user originals) → `docs/knowledge/`;
  `documentkb/` catalog and `codekb/` are derived indexes — dropped.
- **`.claude/` after migration**: `settings.json`, `settings.local.json.example`,
  `agents/`, `aidlc-common/`, `hooks/`, `knowledge/`, `rules/`, `scopes/`,
  `sensors/`, `tools/`, `CLAUDE.md` all delete. If a minimal
  `.claude/settings.json` is still wanted for permissions, rewrite it fresh —
  the current one is 100% AI-DLC wiring.
- **`.cursor/` after migration**: `agents/`, `aidlc-common/`, `cli.json` (its
  only content is the aidlc permission), `hooks/` + `hooks.json`, `knowledge/`,
  `scopes/`, `sensors/`, `skills/`, `tools/` all delete; `rules/` keeps only
  the 7 project `.mdc` files (delete `aidlc.mdc`, `aidlc-onboarding.mdc`, and
  the 4 `aidlc-phase-*.mdc`).

## Sequencing — the in-flight `261027-oxpilot-agent` intent

Recommended: **let `feat/oxpilot-agent` finish and merge under AI-DLC first**,
then run this migration. Rationale: the migration deletes `aidlc/` and all
`.claude`/`.cursor` engine files; if it merges first, the oxpilot branch's
rebase must convert its intent record to `agent-os/specs/_archive/aidlc/261027-oxpilot-agent/`
and continue under the new system mid-flight — doable but noisy.

Status note: draft PR #203 is current — its rebased head now also carries the
last codegen bookkeeping commit (`b9297be6`), and the `tmp/oxpilot-wt`
worktree has been removed, so the branch is the only place that intent lives.

Alternative (if migration must land first): on the oxpilot branch, move the
committed record into `agent-os/specs/_archive/aidlc/261027-oxpilot-agent/`,
delete `aidlc/`, and continue the remaining construction work from the
converted spec. The plan below assumes the recommended order but works either
way — the conversion step is identical, just run on a different branch.

## Work plan

### Phase 0 — This document (done on this branch)

- [x] Research AI-DLC footprint, GSD archive, Agent OS v3 file structure,
      bm-skills, and agentcanon.
- [x] Write `PLAN.md` on `spec/migrate-to-bm-spec-setup`.

### Phase 1 — Agent OS install + vendored skills (no deletions yet)

1. Base install (machine-local, not committed):
   `git clone https://github.com/buildermethods/agent-os ~/agent-os` — pin to a
   commit; record the SHA (there are no release tags; `config.yml` says v3.0).
2. Run `~/agent-os/scripts/project-install.sh` from the repo root → creates
   `agent-os/` + `agent-os/standards/` + `.claude/commands/agent-os/`. Delete
   the `.claude/commands/` tree immediately — commands would be a second,
   Claude-only surface; skills are the single invocation surface (step 4).
3. Vendor `bm-prd-creator`: copy `skills/bm-prd-creator/` from
   `github.com/buildermethods/bm-skills` into `.agents/skills/`; patch its
   output paths `_build_plan/` → `agent-os/product/` (document the patch at the
   top of the vendored file); add a `skills-lock.json` entry with source + hash.
4. Convert the 5 agent-os commands (`commands/agent-os/*.md`) into
   `.agents/skills/agent-os-<name>/SKILL.md` wrappers; add `skills-lock.json`
   entries.
5. Smoke: each new skill folder has valid SKILL.md frontmatter; `git status`
   shows only `agent-os/` + `.agents/skills/` additions.

### Phase 2 — Populate `agent-os/product/` and `agent-os/standards/` from existing setup

6. `product/mission.md` — synthesize from `AGENTS.md` "Product" + GSD
   `PROJECT.md` (core value, one-product cloud+self-host).
7. `product/roadmap.md` — `git mv ROADMAP.md agent-os/product/roadmap.md`
   (decided: single home). Root `ROADMAP.md` becomes a ~3-line pointer file
   ("product roadmap lives at `agent-os/product/roadmap.md`"), not a copy.
8. `product/tech-stack.md` — from `AGENTS.md` stack table + `ARCHITECTURE.md`.
9. `product/decisions.md` — "Decided" sections + learnings embedded in
   `team.md`/`project.md` (with their `cid:` provenance trimmed or noted).
10. `standards/` files per the layout above — distilled from
    `memory/{org,team,project}.md`, `phases/*.md`, `docs/CODE_PRACTICES.md`,
    `docs/VERSIONING.md`, `docs/TESTING.md`, `.cursor/rules/*.mdc`. Keep files
    short; link out to canonical docs rather than duplicating them.
11. Run the `agent-os-index-standards` flow to generate `standards/index.yml`.

### Phase 3 — Migrate spec material into `agent-os/specs/`

12. `git mv docs/archive/planning agent-os/specs/_archive/gsd` (rename inner
    milestone dirs as-is; add a top-level `README.md` in `_archive/gsd`
    noting provenance: "open-gsd planning history, v1.0, archived").
13. For each committed intent: `git mv aidlc/spaces/default/intents/<slug>
    agent-os/specs/_archive/aidlc/<slug>`, then prune gitignored/runtime dirs
    that came along (`.aidlc-engine`, `.aidlc-construction-checkpoints`,
    `runtime-graph.json`) — only committed files move anyway.
14. `git mv aidlc/spaces/default/memory agent-os/specs/_archive/aidlc/memory`
    (raw method files; distilled versions live in `standards/`).
15. `git mv aidlc/spaces/default/knowledge/documents docs/knowledge` if any
    user originals exist (check first — may be empty).
16. Update the 3 referring spots: `AGENTS.md` §Planning, `ROADMAP.md`,
    `docs/CODE_PRACTICES.md` line ~74.

### Phase 4 — agentcanon pass (repo)

17. Remove `.claude/skills/` entirely, then `ln -s ../.agents/skills
    .claude/skills`.
18. Delete `.claude/CLAUDE.md`; add repo-root `CLAUDE.md` symlink → `AGENTS.md`.
    (`.claude/CLAUDE.md` today is the AI-DLC onboarding doc — nothing to keep.)
19. Keep `.cursor/` as-is otherwise — Cursor reads `.agents/skills/` natively
    and its remaining `rules/*.mdc` stay put. Document the convention in
    `AGENTS.md` so new skills land in `.agents/skills/`.

### Phase 5 — agentcanon pass (user-level, decided)

Same convention globally — one real copy in `~/.agents/skills/`, every
harness dir a symlink:

20. `mkdir -p ~/.agents/skills`; consolidate any real skill folders currently
    in `~/.claude/skills`, `~/.codeium/windsurf/skills`,
    `~/.config/devin/skills`, `~/.copilot/skills` into `~/.agents/skills/`
    (inspect first; same-name skills that differ get a manual pick).
21. Replace each of those dirs with a symlink → `~/.agents/skills`
    (`~/.claude/skills`, `~/.config/devin/skills`, `~/.codeium/windsurf/skills`,
    `~/.copilot/skills`).
22. Remove `~/.local/bin/aidlc` and any `~/.aidlc*`/runtime leftovers.
23. (Optional) Install `agentcanon-repo`/`agentcanon-manifest` skills globally
    per the agentcanon repo's instructions.

### Phase 6 — Remove AI-DLC

24. Delete `aidlc/` (incl. untracked `active-memory/`).
25. Delete from `.claude/`: `agents/`, `aidlc-common/`, `hooks/`, `knowledge/`,
    `rules/`, `scopes/`, `sensors/`, `tools/`, `settings.json`,
    `settings.local.json.example`, `CLAUDE.md`.
26. Delete from `.cursor/`: `agents/`, `aidlc-common/`, `cli.json`, `hooks/`,
    `hooks.json`, `knowledge/`, `scopes/`, `sensors/`, `skills/`, `tools/`, and
    `rules/aidlc*.mdc` (6 files).
27. Delete `.agents/skills/aidlc*` symlinks (all point into `.cursor/skills/`).
28. `.gitignore`: drop the AI-DLC block (~L60–118); keep
    `.claude/settings.local.json`. Add nothing for `agent-os/` — all of it
    commits.
29. `AGENTS.md`: rewrite "Planning / AI-DLC" → a short "Planning / Agent OS"
    section (skills live in `.agents/skills`, specs/product/standards under
    `agent-os/`, archives under `specs/_archive/`); delete the whole
    `BEGIN/END AI-DLC:agents` block.

### Phase 7 — Verify + document

30. `grep -ri "aidlc\|AI-DLC" .` returns only `agent-os/specs/_archive/` and
    historical docs (CHANGELOG, archive content) — no live references.
31. `grep -ri "docs/archive/planning"` returns nothing outside archive content.
32. Symlinks valid: `.claude/skills`, `CLAUDE.md`; `bun run`/CI unaffected
    (no build files touched).
33. `make` sanity: no Makefile targets referenced `aidlc` (verified); run
    `make web-lint`-free verification — no code changes, so git-level checks
    + a docs link pass suffice.
34. Update `AGENTS.md` examples (skill table) and `use-in-repo-skills.mdc`
    mapping table if needed to mention `agent-os-*`/`bm-prd-creator` skills.
35. Commit sequence suggestion (SemVer-prefixed subjects):
    `chore(agent-os): install agent-os + vendor bm skills` →
    `docs(agent-os): populate product + standards from aidlc memory` →
    `chore(specs): migrate gsd + aidlc spec archives into agent-os` →
    `chore(agentcanon): canonical skills/instructions layout` →
    `chore(aidlc)!: remove ai-dlc framework` (breaking-ish; `!` optional —
    check `docs/VERSIONING.md` grammar for whether `chore` accepts `!`).

## Decisions

Resolved:

- **One place of truth / agentcanon** — no duplicated surfaces anywhere:
  agent-os commands ship as `.agents/skills/` only (no
  `.claude/commands/agent-os/`), `CLAUDE.md` and `.claude/skills` are
  symlinks, `ROADMAP.md` content moves to `agent-os/product/roadmap.md` with
  a pointer stub at the root, and the same canon applies globally
  (`~/.agents/skills/` real; `~/.claude/skills`, `~/.codeium/windsurf/skills`,
  `~/.config/devin/skills`, `~/.copilot/skills` become symlinks). Global
  `~/.local/bin/aidlc` removal is included in Phase 5.

Still open:

1. **Timing vs `feat/oxpilot-agent`**: recommended path is merge oxpilot first
   (draft PR #203), then migrate. Alternative: convert its intent record to
   `agent-os/specs/_archive/aidlc/261027-oxpilot-agent/` when the branch
   rebases onto post-migration main.
2. **`bm-skill-builder` / other bm-skills**: only `bm-prd-creator` requested;
   `bm-skill-builder`, `bm-design-system`, `bm-favicon-creator` are available
   in the same repo if wanted later.

## Out of scope

- The `feat/oxpilot-agent` feature work itself (continues on its branch /
  draft PR #203; the `tmp/oxpilot-wt` worktree was already removed).
- Any code changes to `apps/`, `crates/`, `packages/` — this is a
  tooling/process migration only.
- Removing unrelated vendored skills or touching `skills-lock.json` entries
  for non-BM skills.
