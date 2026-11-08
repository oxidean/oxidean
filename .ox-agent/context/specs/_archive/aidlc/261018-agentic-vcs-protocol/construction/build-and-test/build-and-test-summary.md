# Build and Test Summary — agentic-vcs-protocol

## Overall build status and prerequisites

Rust workspace + Bun/Turborepo web workspace. `cargo nextest` is the test
runner; `make rpc-gen` owns the generated client; `make web-lint` /
`web-format-check` gate the `apps/web` surfaces. SQLite is the always-on
dialect; pg/mysql lanes run where `DATABASE_URL` is set (NFR6 recorded debt).

## Test type inventory

- Unit tests: per-unit via `cargo nextest run -p <crate>` (all units).
- Integration: cross-unit boundaries (see integration-test-instructions.md).
- Security: credential surface, input validation, integrity bounds, client
  safety (see security-test-instructions.md).
- Performance: NFR1 bound-test family only — no dedicated perf infra
  (recorded debt; see performance-test-instructions.md).
- Web: Vitest integration + Chromium DOM-race project + coverage manifests.
- E2E: `oxidean-cli` wire-shape e2e against a mock transport.

## Coverage expectations

- `oxidean-vcs` (all features): full server+client suite.
- `oxidean-cli`: lib + e2e.
- `oxidean-api`: dispatch/mcp/rpc + auth lanes.
- `apps/web`: Vitest + browser coverage manifests for touched routes.
- Workspace: every crate's suite in one `nextest run --workspace`.

## Target Verification Matrix

| Target ID | Source | Expected | Actual | Evidence | Owning Stage | Verdict |
|-----------|--------|----------|--------|----------|--------------|---------|
| T-NFR1 | requirements.md#NFR1 | flat visit counts on create; caps injectable | Met | test-results.md | build-and-test | Met |
| T-NFR2 | requirements.md#NFR2 | concurrent creates on one parent succeed | Met | test-results.md | build-and-test | Met |
| T-NFR3 | requirements.md#NFR3 | objects+changesets immutable, hash-verified | Met | test-results.md | build-and-test | Met |
| T-NFR4 | requirements.md#NFR4 | identical responses, no shared mutable state | Met | test-results.md | build-and-test | Met |
| T-NFR5 | requirements.md#NFR5 | vcs.* under Oxidean-RPC-Version gate | Met | test-results.md | build-and-test | Met |
| T-NFR6 | requirements.md#NFR6 | 3-dialect migration filename parity | Met | test-results.md | build-and-test | Met |
| T-NFR7 | requirements.md#NFR7 | per-object resumable; verify/temp-rename | Met | test-results.md | build-and-test | Met |
| T-NFR8 | requirements.md#NFR8 | typed vcs.* errors; tracing present | Met | test-results.md | build-and-test | Met |
| T-NFR9 | requirements.md#NFR9 | no cookies on /api/vcs/*; PAT ACL; limits | Met | test-results.md | build-and-test | Met |
| T-SR3 | u1 security-requirements#SR3 | conflicts data ≤500 + truncated flag | Met | test-results.md | build-and-test | Met |
| T-SR4 | u4 security-requirements#SR4.1 | path/mode/author/message/parents bounds | Met | test-results.md | build-and-test | Met |
| T-C1LIM | contract C1 limits | limit ≤500; changes ≤10_000 | Met | test-results.md | build-and-test | Met |
| T-OBJCAP | u3 security-requirements | 256 MiB mid-stream abort; quota probe | Met | test-results.md | build-and-test | Met |
| T-TTL | u6 security-requirements#SR6.5 | import marker TTL 900s; nonce release | Met | test-results.md | build-and-test | Met |
| T-CLI-EXIT | contract C8 / u8 | vcs.unauthenticated/forbidden → exit 3 | Met | test-results.md | build-and-test | Met |
| T-REFNAME | contract C1 / u8 | `ref_name` on wire (req+resp) | Met | test-results.md | build-and-test | Met |
| T-MCP | contract C3 / u9 | 13 tools; owner+name required | Met | test-results.md | build-and-test | Met |
| T-WEB | contract C4 / u10 | toggle panel + vcs.* rows + chips | Met | test-results.md | build-and-test | Met |
| T-BOUNDARY | ADR-011 | client feature: no db/git/sqlx deps | Met | test-results.md | build-and-test | Met |
| T-WS | testing posture | workspace nextest all pass | Met | test-results.md | build-and-test | Met |
| T-WEBSUITE | testing posture | vitest + browser suites pass | Met | test-results.md | build-and-test | Met |
| T-RPCSYNC | affirmed rule | generated client no drift | Met | test-results.md | build-and-test | Met |
| T-XUNIT | step 10 | every FR/NFR/AC covered OK | Met | test-results.md | build-and-test | Met |

## Readiness assessment

**Build-ready, test-ready, deployment-ready.** All executed commands passed and every
applicable target is `Met` — see test-results.md for the finalized matrix and
command evidence.

## Known limitations

- pg/mysql dialect lanes run only where `DATABASE_URL` is set (NFR6 debt).
- No dedicated perf/load infrastructure (NFR1 debt — bound tests are the
  evidence this cut).
- u9 `vcs_changeset_create` `changes` description prose lists an informal
  field shape (`size`, no `old_path`) — prose-only; schema fields themselves
  are correct. Documented in the unit's review record.
