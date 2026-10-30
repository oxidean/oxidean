# U2 Commit Grammar — Functional Spec

## Purpose

Owns the conventional-commit grammar and the one parser every enforcement surface shares (BR2.5). Delivers two things: the in-process `parse_commit` module (U3 consumes) and the `commit-check` subcommand (U5's hook + CI consume). Emits raw signals per C2 — never a bump decision.

## Data model (derived)

- `CommitMessage`: `subject`, `body?`, `changedFiles?`, `source∈{message,file,range,pr_title}`.
- `ConventionalCommit` (C2 output): `sha?`, `type`, `scope?`, `resolved[]`, `scope_status`, `breaking`, `exempt`, `warnings[]`.
- `GrammarRule`: header pattern, breaking markers (`!`, `BREAKING CHANGE:`, `BREAKING-CHANGE:`), exempt subject patterns (`^Merge `, `^Revert "`, `^chore\(release\):`).

## Workflows

### WF-1 `parse_commit` — the shared parse (in-process, library)

1. Exempt pre-check (BR2.3): subject matches an exempt pattern → return `exempt=true`, `valid`, with `type` absent, `scope_status=absent`, `resolved=[]`, `breaking=false`, `warnings=[]` — the exemption swallows everything else. No grammar check — merge/revert/release commits never lint.
2. Grammar check (BR2.1): `type(scope)!: subject`. Unknown type, bad case, missing colon+subject, or multi-token scope → invalid with the offending token named.
3. Breaking detection (BR2.2): `!` in header or BREAKING footer in body → `breaking=true`.
4. Scope resolution (BR2.4): absent → `absent`; registry alias hit → `mapped` + `resolved`; reserved (`release`) → `reserved`; else → `unmapped` + warning.
5. Advisory warnings (Q5): subject >100 chars → warning appended. Warnings never affect validity.
6. Return `ConventionalCommit` — type/scope/breaking/exempt as raw signals; no bump field exists in the type.

### WF-2 `commit-check` — subcommand surface (C2 cli contract)

Modes: `--message <m>` (hook), `--file <path>` (commit-msg file), `--range <a..b>` (CI), `--pr-title <t>` (FR8.1).

1. Collect inputs by mode (`--range` enumerates commits via git log; others take the literal string).
2. Parse each through WF-1 (BR2.5 — identical path).
3. Report: per-commit line on stdout — `commit <sha>: <reason>` for failures, `ok`/`ok (exempt)` for pass; warnings on stderr.
4. Exit: 0 if zero failures (warnings allowed), 1 on any grammar-invalid commit, 2 on usage error (BR2.6, BR2.7).

## States

A parsed commit is `exempt`, `invalid`, or `valid` (+optional warnings). `valid` subdivides by `scope_status` for downstream attribution — `unmapped` is explicitly *valid with warning*, not failure.

## Interfaces consumed by other units

- `parse_commit(msg) -> ConventionalCommit` — U3's ChangeDetector attributes commits through `resolved`/`scope_status`/`breaking`.
- `commit-check` subcommand — U5 wires `.githooks/commit-msg --file`, CI `ci-commit` job `--range` + `--pr-title`, and docs (U7).
- Grammar vocabulary (`allowed_types`, `reserved_scopes`) reads from U1's registry — changing types is a registry edit, not a parser edit.
