# External Dependency Map — Agentic VCS Protocol

A **gated item** is something outside this team's control that a Bolt consumes — external APIs, data windows, approval lead times, another team's hand-off. This build has **none**: every seam the VCS work touches is internal to this repository.

| Item | Type | Blocks | Status |
|------|------|--------|--------|
| `oxidean-db` façade + migrations ×3 | internal crate | B1, B2 | Same repo, same author — no gate. |
| `oxidean-git` backend trait (`Arc<dyn GitBackend>`) | internal crate | B3 | Same repo. |
| `ProtectionChecker` port impl | internal host wiring | B3, B4 | Defined in `oxidean-vcs`, injected by `oxidean-api` — same PR. |
| `proc_classes` / `PROCEDURES` / `system.manifest` registration | internal core | B1, B4 | Same repo. |
| `@oxidean/api-client` regeneration (`make rpc-gen`) | generated artifact | B5 | Same repo — never hand-edited (C9 provenance contract). |
| `apps/web` Octane settings + activity surfaces | internal app | B5 | Same repo. |
| `OXIDEAN_VCS_*` operator env vars | operator surface | runtime | All five defaults frozen in contract-design — no external provisioning. |

## Notes

- CI images (`oxidean-ci:{dev,…}`) already build the workspace; the new crate adds no new toolchain or service dependency.
- Postgres/MySQL/SQLite dialect coverage is exercised by the existing migration-parity harness — no external DB provisioning is gated on a human.
