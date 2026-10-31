# U5 Commit Lint Gates — Business Rules

Source of truth: the fenced YAML block.

```yaml
rules:
  - id: BR5.1
    statement: "The commit-msg hook rejects grammar-invalid subjects (exit 1) and prints the parser's failure line; warnings pass. It is opt-in (installed by `make setup`) and bypassable by `--no-verify`."
    category: policy
    applies_to: HookInstall
    trigger: "git commit when hooksPath is installed"
    logic: "run `commit-check --file $1`; on exit 1 print the failure lines and exit 1; on exit 0 (warnings allowed) pass."
    violation: "commit aborts with the parser's message"
    source: FR1.1
  - id: BR5.2
    statement: "`make setup` sets `core.hooksPath=.githooks`; if a prior foreign hooksPath existed it warns with the old value — it never refuses and never silently clobbers."
    category: policy
    applies_to: HookInstall.install_mechanism
    trigger: "make setup"
    logic: "IF core.hooksPath set and != .githooks THEN warn naming old value; THEN set .githooks."
    violation: n/a
    source: FR1.1
  - id: BR5.3
    statement: "The CI range is the PR's own commits: `base.sha..head.sha` on pull_request events, `before..after` on pushes. The gate never lints commits outside the change being merged."
    category: validation
    applies_to: CommitCheck.range_spec
    trigger: "ci-commit job run"
    logic: "pull_request/synchronize → base.sha..head.sha; push → before..after; workflow_dispatch/manual → --since flag or skip range."
    violation: "exit 1 fails the job"
    source: FR1.2
  - id: BR5.4
    statement: "Every commit in the range AND the PR title are linted through the identical parser path (BR2.5); exempt commits report `ok (exempt)` and never fail."
    category: validation
    applies_to: CommitCheck
    trigger: "ci-commit job run"
    logic: "commit-check --range covers each commit; commit-check --pr-title covers the title. Same parse function — no divergence between local and CI behaviour."
    violation: "exit 1 on any grammar-invalid commit or title"
    source: FR1.2, FR8.1
  - id: BR5.5
    statement: "The `ci-commit` job also runs `versioning check` — registry completeness is enforced in the same gate so a new workspace member cannot merge unregistered."
    category: validation
    applies_to: GateJob.checks
    trigger: "ci-commit job run"
    logic: "commit-check --range AND --pr-title AND versioning check; any failure fails the job; warnings surface but pass."
    violation: "exit 1"
    source: FR1.2, FR2.2
  - id: BR5.6
    statement: "The gate requests only `contents: read` and never pushes, tags, or comments — a pure check job."
    category: constraint
    applies_to: GateJob
    trigger: "job definition"
    logic: "permissions block contains contents:read only; no write scopes, no secrets needed."
    violation: n/a
    source: NFR-security
```

## Rules summary

| Rule | Category | In one line |
|---|---|---|
| BR5.1 | policy | hook blocks invalid subjects; opt-in + `--no-verify` bypass |
| BR5.2 | policy | `make setup` sets hooksPath, warns on foreign value |
| BR5.3 | validation | CI lints the PR's own commit range only |
| BR5.4 | validation | range + title via one parser; exempts pass |
| BR5.5 | validation | registry completeness rides the same gate |
| BR5.6 | constraint | read-only job, no secrets |
