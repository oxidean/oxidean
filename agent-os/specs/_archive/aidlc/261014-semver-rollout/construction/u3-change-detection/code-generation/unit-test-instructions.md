# U3 Unit Test Instructions

Runner: `bun test` (Bun's built-in runner — the same one U1/U2 use).

Exact unit-scoped command:

```bash
bun test scripts/versioning
```

Covers the new U3 tests plus the existing U1/U2 suites in the same tree. The
construction checkpoint verifies with the authorized command:

```bash
make version-check && bun test scripts/versioning && cargo check --workspace
```

Test files added by this unit:

- `scripts/versioning/lib/semver.test.ts` — bump table incl. the 0.x/1.x
  boundary, level ordering, channel suffixes (`-alpha.N`, `-beta.N`,
  `-nightly.YYYYMMDDN` with per-base N restart).
- `scripts/versioning/lib/plan.test.ts` — attribution union
  (scope-only/glob-only/scope_and_glob), exempt-skip, unmapped-scope fallback,
  ambiguous multi-match, transitive propagation + `propagatedFrom` origin set,
  `max(direct, patch)`, fromVersion-from-carrier, entries sorted by id.
- `scripts/versioning/commands/plan.test.ts` + `commands/detect.test.ts` +
  `commands/bump.test.ts` — run against a temp `git init` fixture repo with
  real commits/tags (boundary integration): `--since` bound, per-component tag
  bound, `plan --json` C3 shape, `bump` manifest writes + lockfile regen call.
