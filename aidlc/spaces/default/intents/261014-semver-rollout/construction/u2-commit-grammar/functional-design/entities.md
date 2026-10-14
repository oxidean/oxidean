# U2 Commit Grammar — Entities

Source of truth: the fenced YAML block.

```yaml
entities:
  - name: CommitMessage
    description: Raw input to the parser — a subject line plus optional body and changed-file list.
    attributes:
      - {name: subject, type: string, required: true, constraint: "first line; grammar matched here"}
      - {name: body, type: string, required: false, constraint: "remaining lines; scanned for BREAKING footers"}
      - {name: changedFiles, type: "path[]", required: false, constraint: "populated in --range mode only; unused by the parser, forwarded for context"}
      - {name: source, type: enum, required: true, values: [message, file, range, pr_title]}
    relationships:
      - {to: ConventionalCommit, kind: "1..1", direction: "parses to"}
  - name: ConventionalCommit
    description: The parsed result — the shared output contract C2 hands to U3 and U5.
    attributes:
      - {name: sha, type: "sha40?", required: false, constraint: "present only in --range output"}
      - {name: type, type: "enum?", required: "iff not exempt", values: "registry allowed_types — feat|fix|chore|docs|style|refactor|perf|test|build|ci|revert; absent on exempt results (exemption short-circuits before the grammar check)"}
      - {name: scope, type: "string?", required: false, constraint: "raw token as written, single [a-z0-9._-]+"}
      - {name: resolved, type: "componentId[]", required: true, constraint: "registry alias lookup; empty when unmapped/reserved/absent"}
      - {name: scope_status, type: enum, required: true, values: [mapped, unmapped, reserved, absent]}
      - {name: breaking, type: boolean, required: true, constraint: "true on `!` marker or BREAKING footer"}
      - {name: exempt, type: boolean, required: true, constraint: "merge-subject, auto-revert subject, or chore(release):"}
      - {name: warnings, type: "string[]", required: true, constraint: "unmapped scope, >100-char subject; never fail"}
    constraints:
      - "the parser emits raw signals only — no bump decision lives here (C2; U3 maps signal→bump)"
      - "exempt results carry fixed fields: type absent, scope_status=absent, resolved=[], breaking=false, warnings=[] — the exemption swallows everything else"
    relationships:
      - {to: GrammarRule, kind: "n..1", direction: "validated by"}
  - name: GrammarRule
    description: The versioned conventional-commit grammar — type vocabulary comes from the registry (allowed_types); shape is fixed here.
    attributes:
      - {name: header_pattern, type: regex, required: true, constraint: "^(type)(\\(scope\\))?(!)?: subject$ — single scope token"}
      - {name: breaking_markers, type: "string[]", required: true, values: ["`!` after type/scope", "BREAKING CHANGE:", "BREAKING-CHANGE:"]}
      - {name: exempt_patterns, type: "regex[]", required: true, values: ["^Merge ", "^Revert \"", "^chore\\(release\\):"]}
    constraints:
      - "exempt detection is subject-pattern only — no git parent inspection (mode-independent, Q1)"
      - "types are exact-lowercase; Feat:/FEAT: fail grammar validation (Q5)"
```

## Entity summary

Three entities: `CommitMessage` (input), `ConventionalCommit` (C2 output — type, raw scope, resolved ids, scope_status, breaking, exempt, warnings), `GrammarRule` (the grammar itself — type vocabulary lives in the registry so `allowed_types` changes propagate without a parser edit).
