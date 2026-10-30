# Functional Design — U1 Version Registry — Questions

Unit: `u1-version-registry` (kind: library). Scope: `versioning.toml` schema + loader/validator (`check` subcommand) + manifest literal-version migration. Contract: C1 (contract-summary.md). FRs: FR2–FR2.4 (+ FR2.2 completeness).

## Q1. Registry parse validation — strict or lenient on unknown keys?

- A) Strict — unknown top-level/component keys fail validation (typos can't silently no-op a detection rule).
- B) Lenient — ignore unknown keys for forward-compat.

[Answer]: A — strict. A misspelled `glob` or `depends_on` would silently break detection; forward-compat is handled by additive keys being *valid*, not ignored (contract C1's additive rule covers new fields the schema versioned deliberately). Conductor-decided; ratify at PR review.

## Q2. Completeness check — where does the workspace enumeration come from?

- A) `cargo metadata` + Bun workspace glob from root `package.json`/`bunfig` — authoritative, no hand-maintained list.
- B) Hardcoded component list compared to registry.

[Answer]: A — derive members from `cargo metadata` (workspace members) and the root `package.json` `workspaces` globs; the check compares the derived set to registry entries bidirectionally (unregistered member AND stale entry both fail). FR2.2's guarantee.

## Q3. Baseline commit — recorded how?

- A) `[baseline] commit = "<sha>"` written at implementation time on the merge commit — set once, never rewritten by tooling.
- B) Computed at runtime from tag history.

[Answer]: A — static field in the registry, recorded when the change merges (FR2.5); tag machinery derives `*-v0.1.0` from it. Runtime derivation would break if baseline tags are ever absent locally.

## Q4. Migration mechanics — how do literal versions land?

- A) The implementation edits manifests directly (one-time migration commit); `check` then enforces literals thereafter — the tool itself never runs the migration.
- B) A `migrate` subcommand performs it.

[Answer]: A — one-time manual edit as part of this change (7 Cargo.tomls + 2 package.jsons); no migration machinery to maintain. `check` validates every manifest carries a literal SemVer ≥ the registered kind.

## Q5. `check` output contract beyond C2's exit codes?

- A) `check` prints a machine-and-human report: `ok` per entry on success; `<file>:<line>: <msg>` violations on failure; exit 1; `--json` flag emits the structured violation list for CI annotations.
- B) Plain text only.

[Answer]: A — human text by default, `--json` for CI annotation surfaces (FR1.2's job consumes it).

---

## Consolidated Summary Confirmation

- Strict schema validation (unknown keys fail); workspace members derived from `cargo metadata` + root `package.json` workspaces, checked bidirectionally.
- `[baseline] commit` is a static field set on the merge commit; never runtime-derived.
- Literal-version migration is a one-time manual edit, not tool machinery; `check` enforces it thereafter.
- `check` prints `file:line: msg` violations, exit 1 on failure, `--json` for CI.

Does this all look correct before I generate the functional-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
