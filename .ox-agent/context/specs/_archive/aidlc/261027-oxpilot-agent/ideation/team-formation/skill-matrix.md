# Skill Matrix — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `scope-definition/intent-backlog.md`,
`scope-definition/scope-document.md`, `feasibility/feasibility-assessment.md`.

## Required skills vs. coverage

| Skill | Needed by (proto-units) | Covered by | Gap? |
|---|---|---|---|
| Octane `.tsrx` UI authoring | P-2, P-7 | octane skill + design persona | No |
| `@tanstack/ai-octane` integration (useChat, tools, approvals, SSE) | P-2, P-3 | developer persona + framework docs | Verify in P-2 spike |
| Rust RPC procedures + domain modules | P-1, P-3, P-4, P-5, P-6 | rust-best-practices/rust-async-patterns skills + developer persona | No |
| `oxidean-db` ×3 migrations (pg/mysql/sqlite) | P-1 | developer persona; dialect rules in memory | No |
| SSE/long-lived channels through edge proxy | P-2, P-6 | platform lens + early spike | Watch (R-3) |
| Connector protocol + security/consent design | P-6 | architect + devsecops lens in design stages | Watch (R-1) |
| Transcript redaction/retention + audit | P-8 | compliance lens + devsecops | No |
| Browser/e2e/browser-coverage evidence | P-9 | quality persona + vitest/playwright infra | No |
| Operator docs + env-var documentation | P-1, P-8 | product persona + docs conventions | No |

## Gap analysis

No external skill gap identified. The two watch items (streaming through the
existing edge; the consent/authz/isolation model for computer use) are
knowledge-depth risks inside covered skills — both carry explicit retirement
actions in `feasibility/raid-log.md` (R-3 spike, R-1 dedicated design).

## Remediation plan

None required. If the P-2 spike reveals `ai-octane` integration depth beyond
docs, the developer persona consults the package source/examples directly —
the fallback path is recorded in `build-vs-buy.md`.

## Assumptions & Open Questions

- [assumption] The stage-graph persona set plus in-repo skills is assumed
  sufficient for every proto-unit; if a unit surfaces a genuine specialist
  need (e.g. dedicated security review of the connector protocol), it is
  raised at that unit's stage rather than resourced now.
