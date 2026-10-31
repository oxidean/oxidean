# Architecture Decision Records

One file per decision, numbered `NNNN-kebab-case-title.md`. Numbers are
assigned in merge order and never reused; when a decision is superseded,
mark the old record `Status: superseded by ADR-NNNN` rather than deleting
it — the record stays as provenance.

Format (lightweight Nygard):

```text
# NNNN: Title

- Status: proposed | accepted | deprecated | superseded by ADR-NNNN
- Date: YYYY-MM-DD
- Issue/PR: link

## Context
## Decision
## Alternatives considered
## Consequences
```

Historical context: feature-era decision tags like `D-30`, `D-ACT-13`, or
`BR3.x` in code comments refer to spec documents archived under
`agent-os/specs/_archive/` — they are not ADRs. New cross-cutting
architecture decisions belong here.

## Index

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-cloud-agnostic-storage-strategy.md) | Cloud-agnostic storage strategy for a Git-forge application | accepted |
