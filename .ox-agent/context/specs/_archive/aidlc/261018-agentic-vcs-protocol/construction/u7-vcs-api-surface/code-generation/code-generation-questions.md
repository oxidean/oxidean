# Code Generation — Questions (u7-vcs-api-surface)

## Plan Approval

Approve the code-generation plan and unit-test instructions for `u7-vcs-api-surface`: the `oxidean-vcs` `server`-gated `api.rs` VcsApi orchestration (two-case flag gate, 17 C1 procedure methods, lazy-import trigger on git-oid read misses, total typed error bridge per the C1 taxonomy), `activity.rs` ActivityRecorder (op-granular `repo_activity` rows via the C4 nullable-column mapping), the host `oxidean-api` wiring (17 real RPC dispatch arms replacing stubs, `/api/vcs/{owner}/{name}/objects` GET/POST routes with cookie denial + raised DefaultBodyLimit, host `ProtectionChecker`/`GitBackend` injection into `GitInterop`, `AppState` VCS slot from the frozen `OXIDEAN_VCS_*` defaults, `VALID_PUSH_TYPES` extension from `VCS_ACTIVITY_KINDS`, GC job spawn, factory-reset sweep, MCP Access gating), and the `.env.example`/`docs/CONFIGURATION.md`/compose/`docs/DEPLOYMENT.md`/`.railway` operator wiring — plus the sqlite-backed in-file test suites on both crates.

[Approval Fingerprint]: sha256:v3:0355943dedc6d1721705e51ffc43502ab3d98d2e227b193e7964d1c445915d18
[Planned Source]: 7f556813cb68ae5b82883b59f452e65575400b12eb7d8abb410c5c9ca44bcdb7

1. Approve Plan — proceed to code generation
2. Request Changes — revise the plan

[Answer]: Approve Plan
