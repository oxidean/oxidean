# U2 Commit Grammar — Business Rules

Source of truth: the fenced YAML block.

```yaml
rules:
  - id: BR2.1
    statement: "A valid commit subject matches `type(scope)!: subject` where type is in the registry's allowed_types (exact-lowercase), scope is an optional single `[a-z0-9._-]+` token, `!` is the optional breaking marker, and subject is non-empty."
    category: validation
    applies_to: CommitMessage.subject
    trigger: "every parse"
    logic: "IF the header doesn't match the grammar (unknown type, bad case, malformed scope, missing/empty subject) THEN the parse fails with the offending token named."
    violation: "exit 1 — `commit <sha|input>: <reason>`"
    source: FR1
  - id: BR2.2
    statement: "Breaking intent is signalled by `!` after type/scope OR a `BREAKING CHANGE:`/`BREAKING-CHANGE:` line anywhere in the body (whole-body scan, lenient — a footer spelled mid-body still signals intent)."
    category: validation
    applies_to: ConventionalCommit.breaking
    trigger: "every parse"
    logic: "IF `!` present OR either footer spelling present THEN breaking=true. The parser never downgrades breaking to a bump level — that mapping is U3's job per current version."
    violation: n/a
    source: FR1, C2 raw-signal contract
  - id: BR2.3
    statement: "Exempt commits are detected by subject pattern only: `^Merge `, `^Revert \"`, `^chore\\(release\\):`. No git-parent inspection — hook, CI range-scan, and PR-title modes behave identically."
    category: policy
    applies_to: ConventionalCommit.exempt
    trigger: "every parse"
    logic: "IF subject matches an exempt pattern THEN exempt=true and the parse short-circuits to valid (no grammar check)."
    violation: n/a
    source: FR1, C2
  - id: BR2.4
    statement: "Scope resolution flows through the registry: a scope alias mapping to an entry yields `scope_status=mapped` + `resolved=[ids]`; an unmapped alias yields `unmapped` + a stderr warning (never a failure, detection falls back to paths); the reserved scope `release` yields `reserved` — exempt, never mapped, never warned, never falls back."
    category: policy
    applies_to: ConventionalCommit.scope_status
    trigger: "every parse with a scope"
    logic: "lookup alias → mapped | not in registry → unmapped+warn | == reserved_scopes entry → reserved (exempt). `absent` when no scope token."
    violation: "none — unmapped warns on stderr, exit stays 0 (C2 severity table)"
    source: FR1, FR3, C2
  - id: BR2.5
    statement: "The same parser module drives every caller: `.githooks/commit-msg`, the CI range-scan, `--pr-title`, and U3's in-process detection share one code path — no parallel grammar implementations."
    category: constraint
    applies_to: commit-check subcommand
    trigger: "always"
    logic: "All entry modes (`--message`, `--file`, `--range`, `--pr-title`) funnel through the single parse function; mode only changes where the subject comes from."
    violation: n/a
    source: FR1.3
  - id: BR2.6
    statement: "commit-check output is one failure line per invalid commit on stdout (`commit <sha>: <message>`), warnings on stderr, exit 0 on valid-or-warning, 1 on grammar-invalid, 2 on usage error."
    category: policy
    applies_to: commit-check subcommand
    trigger: "every invocation"
    logic: "per C2 cli contract — stdout lists failures only so CI annotations and hook messages stay machine-greppable."
    violation: n/a
    source: FR1.1, FR1.2, C2
  - id: BR2.7
    statement: "In --range mode every commit in the range is parsed; exempt commits are reported as `ok (exempt)` and never counted as failures."
    category: validation
    applies_to: commit-check --range
    trigger: "CI PR-range scan"
    logic: "FOR each commit in range: exempt → ok line; invalid → failure line; valid → ok. Exit 1 iff any failure."
    violation: "exit 1"
    source: FR1.2
```

## Rules summary

| Rule | Category | In one line |
|---|---|---|
| BR2.1 | validation | strict lowercase conventional grammar, single scope token |
| BR2.2 | validation | `!` or BREAKING footer → breaking signal |
| BR2.3 | policy | exemptions by subject pattern only, all modes identical |
| BR2.4 | policy | scope → mapped/unmapped(warn)/reserved(exempt)/absent |
| BR2.5 | constraint | one parser shared by hook, CI, PR-title, U3 |
| BR2.6 | policy | stdout failures, stderr warnings, exit 0/1/2 |
| BR2.7 | validation | range mode lints every commit; exempts never fail |
