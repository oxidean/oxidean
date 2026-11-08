**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** READY

Scope: U2 code-generation — `lib/commit.ts` parser, `commands/commit-check.ts`,
`cli.ts` registration, tests. Bound to the plan + current source bytes.

### Findings

| ID | Severity | Location | Finding | Resolution |
|---|---|---|---|---|
| F-01 | Minor | commands/commit-check.ts | `--message`/`--pr-title` values containing a newline were passed as subject-only, so `git commit -m $'feat: x\n\nbody'` would fail the header regex. | Split into subject+body before parsing, matching `--file` semantics. |
| F-02 | Minor | commands/commit-check.ts | `runCommitCheck` accepted an unused `deps` param — misleading signature. | Removed; range enumeration stays injectable at the command layer where it is used. |
| F-03 | Minor | lib/commit.ts | `feat:x` (no space after colon) rejects — intentional strictness matching the conventional `type(scope): subject` shape. | Kept; noted in code-summary. |

### Verification

- `bun test scripts/versioning` — 52 pass / 0 fail (22 new for U2).
- Exempt matrix verified: `Merge …`, `Revert "…"`, `chore(release):` all return
  `exempt` with no type — BR2.3.
- Scope statuses mapped/unmapped/reserved/absent verified against a fixture
  registry; unmapped warns without failing — C2 severity table.
- `commit-check` modes: `--message`, `--file`, `--range` (injected log),
  `--pr-title`; exit codes 0/1/2 verified.

### Summary

Parser emits raw C2 signals only (no bump field); all enforcement surfaces
share `parseCommit` (BR2.5); vocabulary reads from the registry. Three minor
items found and fixed before this verdict.
