# Functional Design — U2 Commit Grammar — Questions

Unit: `u2-commit-grammar` (kind: library). Scope: conventional-commit grammar definition, shared parser module, `commit-check` subcommand. Contract: C2. FRs: FR1, FR1.3.

## Q1. Merge-commit exemption — how is a merge detected?

- A) Subject pattern only (`^Merge `) — works identically in `--message`, `--range`, and `--pr-title` modes, no git context needed.
- B) Git parent count when available, subject pattern otherwise.

[Answer]: A — subject pattern only. The hook runs on `commit-msg` where the merge message is what the user wrote; mode-independence keeps hook and CI identical (C2's shared-parser guarantee). `Revert "..."` auto-subjects and `chore(release):` are likewise subject-matched.

## Q2. Scope token shape — single token or lists?

- A) Single scope token `[a-z0-9._-]+` — `feat(a,b):` is invalid, matching upstream conventional-commit practice.
- B) Comma-separated scope lists allowed.

[Answer]: A — single token. `resolved` maps one alias; multi-scope commits are written as multiple commits or use the nearest shared scope. Keeps `scope_status` unambiguous.

## Q3. Breaking-change footers — which spellings count?

- A) Both `BREAKING CHANGE:` and `BREAKING-CHANGE:` (per conventionalcommits.org), anywhere in the body footer block, plus `!` after type/scope.
- B) `BREAKING CHANGE:` only.

[Answer]: A — both spellings + `!`. The parser reports `breaking: true`; U3's planner maps it to major/minor by current version (raw-signal contract).

## Q4. PR-title linting — same rules as commits?

- A) Identical grammar; the merge-commit pattern simply never matches a title. Same `--pr-title` flag → same parse path.
- B) Relaxed rules for titles.

[Answer]: A — identical parse path via `--pr-title`; FR8.1 uses the same subcommand as hook+CI.

## Q5. Case sensitivity and strictness beyond C2?

- A) Types are exact-lowercase; `Feat:`/`FEAT:` invalid. Subject must be non-empty after the colon. Header ≤ 100 chars (advisory warning, not failure).
- B) Case-insensitive types.

[Answer]: A — strict lowercase (conventional grammar); long subjects warn not fail.

---

## Consolidated Summary Confirmation

- Exemptions by subject pattern only (merge/revert/chore-release), mode-independent.
- Single scope token; multi-scope splits into multiple commits.
- Breaking: `!` + both `BREAKING CHANGE:`/`BREAKING-CHANGE:` spellings.
- `--pr-title` reuses the identical parse path; strict lowercase types; ≤100-char subject is a warning.

Does this all look correct before I generate the functional-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
