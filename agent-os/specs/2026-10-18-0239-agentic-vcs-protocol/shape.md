# Agentic VCS Protocol — Shaping Notes (imported)

## Scope

A source-control protocol that does not use git but runs alongside it on
the server for interop — aimed at agentic coding throughput (commit, read,
diff, merge entirely over RPC, no local checkout needed). Logic lives in
one dedicated crate depended on internally by server, CLI, and MCP.

## Decisions

- `oxidean-vcs` single crate with client/server cargo features (client
  default: no DB/git deps)
- Content-addressed object store: sha-sharded CAS, global entry row +
  per-repo link rows; referenced-ness computed at GC time by joining links
  (project-wide CAS convention)
- Changeset model with read index; ref updates are CAS with
  `vcs.ref_conflict{actual}` on non-idempotent retry
- Per-repo flag gate: absent row denies everything including `vcs.status`;
  `enabled:false` denies writes only
- Protocol/machine surfaces authenticate with PAT/OAT only — never session
  cookies
- Git interop is one-directional-safe: export only through
  ProtectionChecker verdicts (fast-forward or deny)

## Context

- **Origin framework:** AI-DLC intent (brownfield, `classic` scope; 5 bolts
  over 10 units, serial unit-major execution)
- **Non-goal boundary:** complements git, never replaces it — real git
  clients keep working (`agent-os/product/roadmap.md` non-goals)

## Outcome

Shipped end-to-end in one PR. Follow-up protocol work (new procedures,
conflict depth, web surfaces beyond the settings toggle) shapes new specs
here.
