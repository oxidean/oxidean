# Commits and versioning

Every commit subject must match the enforced SemVer grammar:

```
type(optional-scope)[!]: subject
```

- Allowed `type`s live in `versioning.toml` `[grammar].allowed_types`
  (`feat`, `fix`, `chore`, `docs`, `style`, `refactor`, `perf`, `test`,
  `build`, `ci`, `revert`).
- `scope` is one token aliasing a `versioning.toml` component; omit it for
  multi-component changes (paths attribute the work).
- `!` marks breaking; `release` scope is reserved for release tooling.
- Exempt subjects: `Merge …`, `Revert "…"`, `chore(release): …`.
- A `commit-msg` hook checks locally (`make setup`); CI `commit-lint`
  checks the PR range and PR title. Subjects drive per-component SemVer
  bumps and changelogs — pick accurate types/scopes.

Each crate, Bun workspace, and the RPC wire contract carries an independent
SemVer starting at `0.1.0`; tags are `<tag_prefix><semver>`. Full grammar,
bump semantics, and release flow: [`docs/VERSIONING.md`](../../docs/VERSIONING.md)
and `.agents/skills/semver-conventions/`.
