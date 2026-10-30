# Decisions

Standing decisions affirmed for this project — distilled from the GSD
milestone record and the AI-DLC memory (`agent-os/specs/_archive/` keeps the
raw decision/audit trail). Do not re-ask these in planning.

## Product

- Dual-mode product, one brand: cloud + self-host share one codebase and one
  Docker-based release train (no hosted-vs-self-host brand split).
- v1 bar is the GitHub-shaped slice; Forgejo/Gitea-class parity work is the
  current backlog ([roadmap](roadmap.md)).
- Auth shipped email+password first; OAuth login and 2FA are backlog
  (SEC-01/SEC-02), not regressions.
- Git transports authenticate with PAT/pubkey only — session cookies are
  never accepted on protocol/machine surfaces.
- Per-repo feature toggles (`repo.issues.*`, `repo.pulls.*`, …): an absent
  flag row denies the surface entirely; `enabled=false` denies writes only,
  reads still succeed.

## Delivery

- Trunk-based development on `main`; every change lands via PR
  (ruleset-enforced — sole bypass actor is the `oxidean-release` GitHub App
  for the stable-cut `chore(release):` commit).
- Squash-merge: linear `main`, one commit per PR; remote branches
  auto-delete.
- Deploy on merge to staging (`images.yml` → GHCR `sha-<sha>` tag set →
  Railway staging). Production is a manual promote of a `main` ancestor with
  green CI (`production-deploy.yml`, `Oxidean / production` environment).
- `railway config apply` runs from operator machines only; apply tokens never
  enter repo or CI.
- Release automation pushes use the `oxidean-release` App or `RELEASE_TOKEN`
  — never `GITHUB_TOKEN`.

## Engineering

- New RPC procedures are `domain.snake_case`; existing mixed-case names are
  wire-stable and stay frozen.
- New subsystems get a trait+adapter seam (`GitBackend`, `EmailSender`
  precedent), not inline code in `oxidean-api`.
- Content-addressed stores use a global entry row + per-repo link rows;
  referenced-ness is computed at GC time by joining links — never a stored
  refcount.
- CLI/server crate splits ship as cargo features (client default = no DB/git
  deps) rather than unconditional deps.
- New API surfaces go under the existing `/api/` prefix (Traefik, Caddy, and
  `oxidean-web` proxy tables already cover it).
- `versioning.toml` registration requires an existing workspace member with
  a literal `[package] version`; `depends_on` edges mirror real Cargo deps
  and are registered in the unit that adds the dependency.
- One semver practice with per-component version numbers (uniform baseline);
  a shared-dependency change counts as a hit on dependents.
- Two-tier changelogs: per-component files plus one consolidated global
  changelog referencing them.
