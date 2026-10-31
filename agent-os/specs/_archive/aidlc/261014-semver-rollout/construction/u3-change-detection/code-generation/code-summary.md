# U3 Change Detection & Bump Engine — Code Summary

## What landed

- `scripts/versioning/lib/semver.ts` — pure version math: bump levels,
  `bumpForCommit` (FR3.1 0.x-aware table — breaking→minor at 0.x, major at
  ≥1.0.0; feat→minor; fix/perf/revert→patch; housekeeping→none), `applyBump`,
  `withChannel` (`-alpha.N`/`-beta.N`/`-nightly.YYYYMMDDN`, per-base N),
  `compareSemver`.
- `scripts/versioning/lib/git.ts` — injectable `Git` interface + `liveGit`:
  `rangeCommits` (`git log`), `commitFiles` (`diff-tree --root`),
  `commitsSince(tag)`, `prefixedTags` (semver tags merged into HEAD).
- `scripts/versioning/lib/plan.ts` — `attributeCommit` (BR3.2 scope∪glob union,
  `scope_and_glob` source, ambiguous multi-match warnings), `propagate`
  (transitive dependent closure with `propagatedFrom` origin ids — BR3.3),
  `carrierVersion` (manifest/constant read, never the registry),
  `computePlan` (C3 `BumpPlan`), `collectRange` (global bound per BR3.1 +
  per-component tag bounds per FR3).
- `scripts/versioning/commands/detect.ts` — `detect [--since] [--json]`
  dry-run attribution listing; shared `parseCommonArgs`/`loadOrFail`.
- `scripts/versioning/commands/plan.ts` — `plan [--since] [--channel] [--json]`;
  `--json` emits exactly `{schema, version, generatedAt, baseRef, entries}`.
- `scripts/versioning/commands/bump.ts` — applies the same `buildPlan` path:
  carrier writes (Cargo `[package] version`, package.json `"version"`,
  `constant_path` literal) + unconditional `cargo metadata` /
  `bun install --lockfile-only` regen; `--dry-run` prints without writing.
- `cli.ts` — `detect`/`plan`/`bump` registered.

## Decisions

- **Per-component bound (FR3 vs BR3.1):** the scan range is global
  (`--since`/`baseline`/root-while-pending), then each component with a prior
  `<tag_prefix>*` tag drops commits reachable from that tag — "since its
  previous tag" wins per component.
- **Grammar-invalid commits** in the range still attribute via globs and count
  as patch-level evidence (warning emitted) — fail toward releasing; U5's hook
  keeps them off main in practice.
- **Reserved scopes** (`release`) produce zero attribution — including no glob
  fallback (BR3.2: "scope_status=reserved → nothing").
- **Origins** are components whose direct bump is non-none — a chore-only dep
  does not propagate (FR3.2 "a *bumped* internal dependency").
- `propagatedFrom` records attributed origin ids (not intermediates), and
  origin→origin edges are kept as evidence for FR4.4's Dependencies section.
- Package.json writes match on the exact current literal to avoid clobbering
  nested `version` keys; Cargo.toml writes only inside `[package]`.

## Tests

30 new tests (82 total under `bun test scripts/versioning`): bump table incl.
0.x/1.x boundary, channel suffixes, attribution union/ambiguity, propagation
closure + transitive origins, `max(direct, patch)`, per-component tag bounds,
C3 shape, plus a real `git init` fixture repo covering `detect`, `plan
--json`, `--since` bounds, `bump` writes and lockfile regen end to end.
