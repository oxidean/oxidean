# Oxidean

## What This Is

Oxidean is a GitHub-shaped forge: git hosting, issues, pull requests, reviews, Actions-compatible CI, packages, and social discovery. The same product runs as Oxidean Cloud and as self-hosted software on your own infrastructure.

## Core Value

One forge you can trust in the cloud or on your own machines — without splitting into separate “hosted brand” vs “self-host software” products.

## Current State

Shipped **v1.0 MVP** (2026-09-19): 24 phases (1–22 + 11.1 + 22.1), 220 plans, 87 v1 requirements Complete. Stack is Rust Axum RPC + Octane TanStack Start (`.tsrx`), ShadCN/Base UI/Tailwind v4, multi-DB (SQLite/Postgres/MySQL), system-git `GitBackend`, Compose CI matrix, Railway-class deploy path. Branch protection (including direct-push denial) is packaged in the Compose/API image.

## Next Milestone Goals

Define via `/gsd-new-milestone`. Likely themes from deferred v2 backlog: OAuth/2FA, boards/wiki/CODEOWNERS/merge queues, cross-repo search, email digests, federation, fine-grained OAuth apps, mobile. Immediate hygiene debt: wire `smoke-protection` into CI; raise coverage floor; live Railway apply remains human-verify.

## Requirements

### Validated

- ✓ Web UI: OctaneJS via `@octanejs/tanstack-start`, ShadCN + Base UI, Tailwind CSS v4 — v1.0
- ✓ Backend services in Rust — v1.0
- ✓ Shared types via RPC codegen (Rust → TypeScript) — v1.0
- ✓ App data: SQLite, PostgreSQL, and MySQL — v1.0
- ✓ Git via system `git` CLI + `GitBackend` / filesystem storage — v1.0
- ✓ Email/password auth, sessions, profile — v1.0
- ✓ Email: log sink / SMTP / Resend — v1.0
- ✓ Cloud email verify before privileged actions; `allow_signup` gate — v1.0
- ✓ Self-host admin bootstrap (env or wizard) + factory reset — v1.0
- ✓ Git hosting over HTTPS (PATs) and SSH — v1.0
- ✓ Orgs, roles, collaborators, visibility ACL — v1.0
- ✓ Issues, PRs (review/merge strategies), branch protection — v1.0
- ✓ Actions-compatible CI + official runner image (no managed minutes) — v1.0
- ✓ Git LFS, releases, rename/transfer, in-repo search — v1.0
- ✓ Webhooks + in-app notifications — v1.0
- ✓ Packages registry (OCI + npm + generic/raw) — v1.0
- ✓ Social explore (stars, profiles, forks) — v1.0
- ✓ Docker Compose local/self-host + CI Compose matrix — v1.0
- ✓ Railway-class cloud deploy path (same images) — v1.0
- ✓ Brand shell + system/light/dark theme — v1.0
- ✓ Branch protection direct-push denial in Compose/API image — v1.0 (Phase 22.1)

### Active

<!-- Next milestone — filled by /gsd-new-milestone -->

- [ ] OAuth login (GitHub and/or Google) — was AUTH-V2-01
- [ ] Two-factor authentication — was AUTH-V2-02
- [ ] Project boards / kanban — was COLLAB-V2-01
- [ ] Wiki per repository — was COLLAB-V2-02
- [ ] CODEOWNERS-enforced reviews — was COLLAB-V2-03
- [ ] Merge queues — was COLLAB-V2-04
- [ ] Instance-wide / cross-repo code search — was COLLAB-V2-05
- [ ] Email notification digests — was COLLAB-V2-06
- [ ] CI `smoke-protection` regression guard (ORG-06) — tech debt from v1.0 audit
- [ ] Coverage floor ratchet toward ~0.70; CI llvm-cov collect — WINDOWS 53–54

### Out of Scope

- Replacing git with a custom VCS — stay git-compatible so existing workflows work
- Forking Gitea/Forgejo as the product identity — Oxidean is its own brand and codebase direction
- Separate cloud-only vs self-host-only feature forks — dual-mode means one product
- Vercel as the forge app runtime — git/SSH/stateful services need containers; Docker is the unit of deploy
- OAuth in v1 — shipped email/password; OAuth remains next-milestone Active
- Invite-only cloud gate in v1 — `allow_signup` after bootstrap; invite codes deferred
- Federation / ActivityPub — PLAT-V2-01
- Official mobile apps — PLAT-V2-03

## Context

- **Brand:** Oxidean = octane (performance / octa wink) + nest (where repos live). Chosen over Octabase (collides with AFFiNE’s OctoBase) and Octahub (too GitHub-formula + OctoHub collisions).
- **Logo (current):** [`brand/oxidean-mark.png`](../brand/oxidean-mark.png) — four-arrow X mark, blue (cool/left) + orange (warm/right) on black. Use as primary mark (UI, favicon, README) until a vector set exists.
- **Brand colors (from mark):** cool blues/cyans vs warm oranges; high-contrast on dark surfaces.
- **Model:** GitLab-style dual-mode (one product, cloud + self-host), not Codeberg/Forgejo split (hosted instance vs different software brand).
- **Stack (shipped):** **Rust** API (`oxidean-api` / core / db / git); **Octane** web (`.tsrx`) on `@octanejs/tanstack-start`; ShadCN + Base UI + Tailwind CSS v4; typed RPC client `@oxidean/api-client` via `make rpc-gen`.
- **Source control (working):** [`git@github.com:oxidean/oxidean.git`](https://github.com/oxidean/oxidean) — org `oxidean`, repo `oxidean`.
- **v1.0 scale:** ~2057 files changed from scaffold to close; ~10 calendar days (2026-09-09 → 2026-09-19).
- **Known residual debt:** CI omits `smoke-protection`; WINDOWS open_count 2; Nyquist partial on several mid phases; Class C deferred (GlobalSearch depth, issue_comment webhook, Checks tab, follow/watch, live Railway apply).

## Constraints

- **Brand:** Keep the octa/octane connection; never ship as “GitHub clone” branding
- **Distribution:** Cloud and self-host must share one codebase and one Docker-based release train
- **Compatibility:** Real git clients and remotes must work (`git@…:user/repo.git`)
- **Runtime:** Docker Compose is the supported way to run Oxidean (local, self-host, and cloud)
- **Cloud host:** Container platform such as Railway (same images as local Compose)
- **Self-host bootstrap:** If `OXIDEAN_ADMIN_EMAIL` and `OXIDEAN_ADMIN_PASSWORD` are set, create that admin on first boot. Otherwise show a one-time setup wizard to create the admin, then normal signup rules apply for the instance.
- **UI:** OctaneJS on TanStack Start (`@octanejs/tanstack-start`); ShadCN + Base UI components; Tailwind CSS v4 with CSS-first configuration
- **Theme:** Light and dark; default = system preference; user can lock light or dark
- **Backend:** Rust for forge/API/git-facing services
- **Type safety:** Rust → TypeScript via RPC + codegen (rspc / specta-style); procedures and types stay in sync with watch-friendly regen in development
- **App database:** SQLite, PostgreSQL, and MySQL all supported via one storage abstraction; instance chooses dialect through config/env
- **Git engine:** **System `git` CLI (≥2.5)** via **`GitBackend` / `CliGitBackend`** with repos on filesystem volumes; document a future **gitoxide/`gix` (`GixGitBackend`)** adapter when feature coverage allows
- **Email:** Unconfigured → log/dev sink; configured → **SMTP** and **Resend** adapters
- **Project CI:** Docker Compose build + validation on every PR (the path that ships)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Product name: **Oxidean** | Octane DNA + nest metaphor; softer collisions than Octabase/Octahub | ✓ Good |
| Dual-mode (cloud + self-host) | Compete with GitHub while letting people run their own instance | ✓ Good |
| One brand for both modes | Avoid Codeberg/Forgejo-style brand split | ✓ Good |
| GitHub remote: **oxidean/oxidean** | Working source-control home while product brand is Oxidean | ✓ Good |
| v1 bar: **GitHub-shaped slice** | Thin forge + Actions CI + packages/registry + explore/social | ✓ Good |
| Runtime: **Docker Compose everywhere** | One container stack for local, self-host, and cloud | ✓ Good |
| Cloud host: **Railway (or equiv.)** | Host the same Compose/images; not a second app architecture | ✓ Good (IaC + docs; live apply human-verify) |
| Dropped: **Vercel as forge runtime** | Stateful git needs containers; Docker is enough | ✓ Good |
| Project CI: **Compose validation** | CI proves the path operators actually run | ✓ Good |
| Auth v1: **email + password** | Ship sessions first; OAuth later | ✓ Good |
| Cloud signup: **open + email verify** | Public competitor feel; verify before privileged actions | ✓ Good |
| Self-host admin: **wizard, or env if set** | Compose-friendly override; safe default for empty installs | ✓ Good |
| UI: **OctaneJS + TanStack Start** | `@octanejs/tanstack-start` app shell | ✓ Good |
| UI kit: **ShadCN + Base UI + Tailwind v4** | CSS-based Tailwind config; component system | ✓ Good |
| Theme: **system default, light/dark override** | Respect OS; allow explicit preference | ✓ Good |
| Backend: **Rust** | Performance/correctness for git-heavy forge services | ✓ Good |
| Types: **RPC + codegen (rspc/specta-style)** | End-to-end procedure + type safety; watch regen in dev | ✓ Good |
| App DB: **SQLite + Postgres + MySQL** | Operator choice; one abstraction, three dialects | ✓ Good |
| Git engine: **CLI-primary + GitBackend** | System `git` now (`CliGitBackend`); future `GixGitBackend` when coverage allows (D-32) | ✓ Good |
| Email: **log sink / SMTP / Resend** | Safe default locally; real relays when configured | ✓ Good |
| Git HTTPS: **PATs only** | No account password over git; create/revoke in UI | ✓ Good |
| PR merges: **merge / squash / rebase** | All three strategies; per-repo enable/disable | ✓ Good |
| Packages: **OCI + npm + generic/raw** | Full registry surface for common publish needs | ✓ Good |
| Actions: **official runner image + 3rd-party protocol** | Operator-hosted compute; Blacksmith-class providers can integrate; no managed minutes in v1 | ✓ Good |
| v1 extras: **LFS, webhooks, notifs, search, releases, transfer, branch protection** | Full GitHub-shaped collaboration surface | ✓ Good |
| Roadmap: **fine granularity (22 phases)** | Thin slices for parallel planning/execution | ✓ Good |
| Logo: **brand/oxidean-mark.png** | Current brand mark (blue/orange X) | ✓ Good |
| Phase 3: **semantic tokens + PWA shell** | Squircle mark, system/light/dark, assets-only SW | ✓ Good |
| ORG-06 packaging: **update hooks in Compose/API image** | Direct push must deny on protected branches in shipped images | ✓ Good |
| Verification close: **fingerprint refresh, tests-as-proof** | Avoid 22× conversational UAT when VERIFICATION + suites already green | ✓ Good |

---
*Last updated: 2026-09-19 after v1.0 milestone*
