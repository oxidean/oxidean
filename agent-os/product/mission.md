# Mission

Oxidean is a self-hostable code forge — git hosting, issues, pull requests,
orgs, LFS, releases, packages, and Actions-compatible CI — delivered as **one
product** that runs as Oxidean Cloud and as self-hosted software on the
operator's own infrastructure.

## Core value

One forge you can trust in the cloud or on your own machines — without
splitting into separate "hosted brand" and "self-host software" products.
GitLab-style dual-mode: one codebase, one Docker-based release train, real
git clients and remotes keep working.

## What shipped

v1.0 (2026-09-19): the full GitHub-shaped collaboration slice plus Compose
self-host and a Railway IaC deploy path — Postgres, MySQL, and SQLite via one
storage abstraction. Details: [`agent-os/product/roadmap.md`](roadmap.md)
and the [v1.0 milestone archive](../specs/_archive/gsd/).

## Non-goals

- Replacing git with a custom VCS.
- Forking Gitea/Forgejo as the product identity — Oxidean is its own brand.
- Separate cloud-only vs self-host-only feature sets.
- Non-Docker runtime targets for the forge app.
- Managed CI runner minutes on Oxidean Cloud (self-hosted runners by design).
