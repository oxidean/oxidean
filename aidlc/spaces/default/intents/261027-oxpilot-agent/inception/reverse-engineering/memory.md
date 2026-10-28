# Memory — Reverse Engineering (Inception)

- Store verdict STALE (built by 261018-agentic-vcs-protocol, analyzed `./`); conductor chose **full rescan** — the feature spans apps/web + crates + packages + docs and a fresh full store serves every future intent.
- Snapshot: store_generation `sha256:4daeb29c…`, source_fingerprint `git:9ed944bd…` (HEAD 6c36922a).
- Unrecorded repo → no `--repo` qualification; handoff at `inception/reverse-engineering/developer-scan.md`.
- Developer link dispatched (background); architect link follows once handoff is verified.
- Developer scan written `developer-scan.md` @ HEAD `6c36922a` (main): full rescan, post-#162 architecture (36 crates, 469 procs), SSE/`settings/ai`/admin.mcp/PAT-fail-closed precedents documented; CodeKB 9-artifact staging + `codekb-publish` still pending (mint fingerprint via `aidlc engine workspace codekb-scope-diff --mint --paths ./`).
- Architect published full store: generation `sha256:c4c01510…`, compare COVERS, `kind: full` over `./`, HEAD 6c36922a. Staging + scope-draft cleaned.
- Concern carried forward: streaming transport is the design fork — dedicated `GET /api/oxpilot/…` SSE route (job-log precedent) while `oxpilot.*` control plane rides typed RPC in a new `oxidean-api-oxpilot` domain crate; new procs fail-closed `SessionOnly` for PATs (desired v1).
- Also carried: feature flag clones `OXIDEAN_MCP_ENABLED`+`admin.mcp.*`; transcripts in `oxidean-db` (migrations ×3); connector protocol is the only greenfield surface; every addition has paired artifacts (route-coverage row, SHELLS row, env docs).
