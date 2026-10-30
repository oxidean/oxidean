# U1 Version Registry — Unit Test Instructions

## Framework

- Runner: **Bun test** (`bun:test`), consistent with how `scripts/*.ts` execute
  under Bun. No vitest config needed — `scripts/` is outside the turborepo
  workspaces.
- Language: TypeScript (Bun-native, no build step).

## Exact unit-scoped command

```bash
bun test scripts/versioning
```

Runnable before the first test step: Bun is already the repo toolchain
(`bun.lock`, `bun run` throughout the Makefile). Verify once with
`bun --version`.

## Test files

- `scripts/versioning/lib/registry.test.ts` — schema + validator unit tests
  (in-memory TOML strings via temp fixture dirs).
- `scripts/versioning/commands/check.test.ts` — check-runner unit tests with
  injected workspace enumeration (no real `cargo metadata` call).
- `scripts/versioning/check.integration.test.ts` — executes
  `bun scripts/versioning/cli.ts check` against the real repository root;
  asserts exit 0 and the 10-entry ok summary. Marked so it is the only test
  that shells out.

## Coverage targets (Standard strategy: 5–8 tests per component)

`lib/registry.ts` (~8 tests):
1. valid minimal registry parses to typed `RegistryConfig`
2. unknown key at top level → violation naming the key path
3. unknown key inside an entry → violation
4. `kind=crate` missing `manifest_path` / carrying `constant_path` → violation
5. `kind=contract` missing `constant_path` → violation
6. duplicate scope alias / tag_prefix across entries → violation
7. overlapping globs across entries (`crates/**` vs `crates/oxidean-api/**`)
   → violation; disjoint globs pass
8. `depends_on`: unknown id, self-edge, and cycle → violation each
9. `baseline_commit` absent / non-sha40 / `pending` → fail, fail, warn-ok

`commands/check.ts` (~6 tests):
10. complete fixture workspace → exit 0, per-entry ok lines
11. workspace member with no entry → exit 1 naming the member
12. entry pointing at missing manifest → exit 1 naming the entry
13. manifest with `version.workspace = true` (non-literal) → exit 1 (BR1.1)
14. `--json` emits structured `{file,line,message}` list
15. usage error (unknown subcommand) → exit 2

Integration (1 test):
16. real repo: `bun scripts/versioning/cli.ts check` exits 0 and lists 10
    entries.

## Mocking / fixtures

- Fixture workspaces are constructed in `tmp()` dirs: minimal `Cargo.toml`
  sets + `package.json` trees; workspace enumeration is injected as data for
  unit tests so `cargo metadata` never runs there.
- The integration test is the only place the real repo + `cargo` is touched;
  skip it if `cargo` is absent (guard with `spawnSync` check).

## Test data management

- Each test builds its own temp dir under `node:os.tmpdir()` and removes it in
  `afterEach`/`finally`.
- No shared fixtures; no network; no writes outside temp dirs except the
  read-only integration check.
