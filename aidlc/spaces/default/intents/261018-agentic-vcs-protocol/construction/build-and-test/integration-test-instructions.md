# Integration Test Instructions — agentic-vcs-protocol

Standard strategy: cross-unit interaction and external-dependency handling.
All commands run from the repo root.

## Framework setup

- Rust: `cargo nextest` against the workspace; per-crate filters scope runs.
- Web: Vitest (happy-dom integration project) + Chromium browser project.
- CLI end-to-end: `crates/oxidean-cli/tests/commands_e2e.rs` drives `ox` against
  a mock RPC transport asserting exact wire bodies.
- Docker: `make up-with-dev-auth` for a real stack when manual smokes are
  needed (Compose + Mailpit/OIDC stubs).

## Cross-unit boundaries exercised

1. **Wire ↔ persistence (u1↔u2):** DTO round-trips land in `vcs_*` tables;
   nullable repo-activity columns generalized for `vcs.*` rows.
2. **Object store ↔ changeset core (u3↔u4):** `changeset_create` persists
   manifest entries referencing stored blobs; missing blobs surface typed
   `vcs.not_found`, never corrupt rows.
3. **Merge engine ↔ store/interop (u5↔u4/u6):** merge-base walks the persisted
   DAG; two-pass `merge3` writes zero rows on deterministic abort.
4. **Git interop ↔ protection port (u6↔u7):** `vcs.git_export` moves the branch
   through the injected verdict path — protected denials block export exactly
   like a push; `vcs.git_import` and lazy import converge on identical
   changeset ids.
5. **API surface ↔ auth/manifest (u7↔u1):** `proc_classes` read/write/admin
   split; `SessionOnly` fail-closed on unlisted names; `vcs.disabled` on
   writes for never-enabled/disabled repos; archived repos read-only;
   `repo_activity` `vcs.*` rows on mutations.
6. **CLI ↔ transport (u8↔u7):** `CliVcsTransport` rides `rpc::Client` for
   `vcs.*` and raw object routes for GET/PUT; `vcs.ref set` sends `ref_name`
   with CAS `expected_old`/`new` nulls; exit code 3 on `vcs.unauthenticated`/
   `vcs.forbidden`; `required_procedures`/`CLI_PROCS` fail-closed vs manifest.
7. **Working copy ↔ object routes (u8↔u3/u7):** sparse `PathFilter` checkout
   pages `vcs.tree`, fetches want-miss blobs via object GET, verifies hashes
   into the bounded cache, materializes temp-then-rename, refuses non-empty
   dirs without the `.oxidean-vcs-checkout` marker.
8. **MCP ↔ RPC (u9↔u7):** 13 descriptors thin-map to same-named `vcs.*`
   procedures; `owner`/`name` required per schema; `vcs_log.from` default
   injection; RepoRead/RepoWrite classes enforced by `enforce_pat_scope`.
9. **Web ↔ API (u10↔u7):** `VcsSettingsPanel` toggle mutation + activity-row
   `vcs.*` rendering + per-kind filter chips; `cs_<ulid>` ids link to the
   changeset page while git-oid exports do not.

## Commands

```bash
cargo nextest run --workspace                 # everything below is a subset
cargo nextest run -p oxidean-vcs --all-features
cargo nextest run -p oxidean-cli              # lib + commands_e2e
cargo nextest run -p oxidean-api              # dispatch/mcp/rpc integration
bun run --filter @oxidean/web test            # Vitest unit+integration
bun run --filter @oxidean/web test:browser    # Chromium DOM-race suite
make rpc-sync-check                           # generated-client drift gate
make browser-coverage-check-pr                # route/manifest coverage
```

## Expected coverage

- `oxidean-vcs`: 207+ tests (domain, object store, merge, interop, client,
  CLI surface, MCP descriptors).
- `oxidean-cli`: 130 lib + 25 e2e (incl. `vcs.*` wire-shape asserts).
- `oxidean-api`: full suite incl. vcs dispatch, object-route auth, MCP wiring.
- `apps/web`: 609 Vitest + Chromium panel test.
- Workspace aggregate: 1529 passing / 3 skipped.

## Environment notes

- SQLite is the always-on dialect; `DATABASE_URL` lanes add pg/mysql where CI
  executes them (skip-when-unset is recorded debt — NFR6).
- Browser tests need Chromium via the project's Vitest browser provider.
