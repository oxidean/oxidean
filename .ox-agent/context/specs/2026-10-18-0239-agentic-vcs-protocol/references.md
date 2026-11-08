# References — Agentic VCS Protocol

## Code surfaces

- `crates/oxidean-vcs` — protocol core: CAS object store, changesets, merge
  engine, git import/export
- `crates/oxidean-api-vcs` — `vcs.*` RPC + object route surface
- `ox vcs` commands — `crates/oxidean-cli` (16 commands + WorkingCopy)
- MCP tools — instance MCP server surface
- Web: repo-settings VCS toggle; `vcs.*` activity rows in the repo feed

## Archive + provenance

- AI-DLC intent:
  [`../_archive/aidlc/261018-agentic-vcs-protocol/`](../_archive/aidlc/261018-agentic-vcs-protocol/)
  — `inception/delivery-planning/bolt-plan.md` has the full unit DAG and
  per-bolt definitions of done
- Shipped: PR #159
