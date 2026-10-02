# Oxidean

<p align="center">
  <img src="brand/oxidean-mark.png" alt="Oxidean" width="128" height="128" />
</p>

<p align="center">
  <a href="https://github.com/oxidean/oxidean/actions/workflows/ci.yml"><img src="https://github.com/oxidean/oxidean/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT" /></a>
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/version-0.1.0-informational.svg" alt="Version 0.1.0" /></a>
</p>

## What it is

Oxidean is a self-hostable code forge: git hosting, pull requests, issues, CI actions, and package registries in **one product**. The same codebase runs Oxidean Cloud and installs on your own machines.

## Who it's for

- **Operators** who want a Compose-deployed forge under their own control
- **Teams** that want the full repository workflow without splitting cloud and self-host into different products
- **Contributors** improving the single codebase behind both deployments

## Cloud vs self-host

**Oxidean Cloud** and **self-hosted Oxidean** share the same images and application. Self-host with Docker Compose today; Cloud is the hosted instance of that same stack. See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

## Quick start

Prerequisites: Docker Engine with Compose v2, and a free host port **80** (Traefik). Git over SSH also publishes host port **2222** by default.

```bash
cp .env.example .env
make up
make smoke
```

Open [http://localhost](http://localhost). On a fresh install, `/setup` walks you through creating the admin account and choosing auth providers. Tear down with `make down`.

Other database dialects:

```bash
make up-mysql && make smoke-mysql
make up-sqlite && make smoke-sqlite
```

Full walkthrough (including host-side development): [docs/GETTING-STARTED.md](docs/GETTING-STARTED.md).  
Production-oriented Compose notes: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

## What's included

Shipped on the current mainline:

- **Git hosting** over HTTPS (Smart HTTP + PATs) and SSH, with commit signature verification (SSH and GPG) and author recognition
- **Code browsing**: tree, blob, blame, commits, compare, branches, tags, and a repo activity feed
- **Pull requests** with merge controls, plus **branch protection** rules
- **Issues** with comments, labels, and assignees
- **CI actions**: workflow runs and logs, with self-hosted runners (`oxidean-runner`)
- **Package registries** for OCI, npm, and generic/raw artifacts
- **Git LFS** with quotas, **releases** with assets, repo rename and transfer
- **Notifications**, repo **webhooks**, and repository **forks**
- **Search** across repos, code, issues, users, and orgs, plus an **explore** feed with stars, watchers, and social lists
- **Organizations**, collaborators, invites, and repository visibility / ACL
- **Repository mirroring** (push, pull, and two-way) and **stack presets** on `/new`
- **Auth**: local accounts plus SSO via OIDC or WorkOS AuthKit; fine-grained PATs and SSH keys in user settings
- **Admin console**: auth providers, LFS, packages, runners, and repo templates, with a first-run setup wizard and factory reset
- **Multi-database** support: Postgres, MySQL, SQLite

What still separates Oxidean from established forges is tracked in [ROADMAP.md](ROADMAP.md).

## Docs

Canonical docs live under [`docs/`](docs/).

**Run & operate**

- [`docs/GETTING-STARTED.md`](docs/GETTING-STARTED.md) — first run (Compose or host)
- [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) — Compose images, Traefik, volumes, production knobs
- [`docs/CONFIGURATION.md`](docs/CONFIGURATION.md) — environment variables
- [`docs/database.md`](docs/database.md) — Postgres / MySQL / SQLite, migrations, dialect switching

**Product & API**

- [`ROADMAP.md`](ROADMAP.md) — remaining work toward functional forge parity
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — system overview (git, SSH, LFS, orgs, packages)
- [`docs/API.md`](docs/API.md) — RPC + Smart HTTP / LFS / SSH / registry surfaces
- [`docs/CLI.md`](docs/CLI.md) — `ox`, the companion command-line client
- [`docs/guides/stack-presets.md`](docs/guides/stack-presets.md) — in-repo `/new` stack presets

**Develop**

- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — local API + Vite, RPC sync, monorepo layout
- [`docs/TESTING.md`](docs/TESTING.md) — nextest + Vitest + stack e2e
- [`docs/dev-auth.md`](docs/dev-auth.md) — auth/email stubs without cloud secrets
- [`docs/CODE_PRACTICES.md`](docs/CODE_PRACTICES.md) — conventions for humans and agents

## Contributing

Bugs and feature requests go through the issue templates on this repository. To change the code, start with [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/CODE_PRACTICES.md](docs/CODE_PRACTICES.md); agents also read [AGENTS.md](AGENTS.md) (Octane ≠ React). Security reports follow [SECURITY.md](SECURITY.md), not public issues.

## License

[MIT](LICENSE) © Oxidean contributors
