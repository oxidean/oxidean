# U4 Changelog & Release — Unit Test Instructions

Contract: `test-after`. Strategy: `standard` (5–8 tests per component).

## Suite

`bun test scripts/versioning` stays green; new tests in
`scripts/versioning/lib/changelog.test.ts` and
`scripts/versioning/commands/release-tags.test.ts` plus a workflow-shape file.

## Cases

1. Changelog render: entries map to buckets (feat→Features, fix→Fixes,
   perf→Performance, revert→Reverts, propagated→Dependencies); `## [v] - date`.
2. Idempotent upsert: regenerating the same version replaces the section,
   never duplicates; other versions' sections untouched.
3. Stub creation: missing changelog file gets header + section.
4. Root consolidation: root CHANGELOG.md gains a release section linking each
   component tag.
5. `release-tags`: temp git repo → annotated `<prefix><ver>` tags created with
   the section as annotation; existing tags skipped; `--all` tags every
   component at carrier version; missing tag refs exit non-zero on failure.
6. Workflow shape: release.yml excludes `ox-v*`/`oxidean-runner-v*`, marks
   prereleases, gates `latest.json` to stable; ox-cli.yml triggers on `ox-v*`
   and carries `crates/oxidean-core/**`; publish-runner triggers on
   `oxidean-runner-v*`.

## Boundaries

- Tag tests run against a temp git repo; nothing pushes.
- Workflow coverage is static YAML assertions; `act` verification lives in
  Build & Test.
