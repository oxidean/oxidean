## Review

**Stage:** code-generation
**Unit:** u7-vcs-api-surface
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

None — the prior pass's findings (bare `internal` wire codes, full-object buffering in `object_get`, the `map_auth` doc overstatement) are integrated in the bound bytes: all seven sites now emit `rpc.internal`, `object_get` streams via `futures_util::stream::unfold` in 64 KiB chunks with error-terminating state, and the `map_auth` comment matches the code.

### Summary

Review pass over the post-integration bytes: `oxidean-vcs/src/{api,activity}.rs` (17 procedures, three-case flag gate, CAS, lazy-import retry, activity recording, typed error bridge), `oxidean-api/src/{vcs,vcs_routes,vcs_protection}.rs`, the `rpc.rs` dispatch arms + `RpcCtx` slot, `app.rs` `build_vcs_slot` + route mount + `DefaultBodyLimit`, the GC job + factory-reset sweep, `proc_classes` route aliases, `main.rs` 8 MiB worker stacks, `.cargo/config.toml` test stacks, env/docs wiring.

Verified: tier resolvers (`resolve_repo_for_admin`/`resolve_repo_for_owner_mutate`/`resolve_repo_for_read`) match the C1 tier table and cover org-shared repos via capability; flag gate three-case on RPC + two-case on routes per AC1.2.1; Bearer-only object routes with PAT route-alias classification and private-repo 404 anti-enumeration; `quota_exceeded`→413 with body-limit headroom; `HostProtectionChecker` consumes `evaluate_push` verdicts without leaking internals; lazy retry yields `vcs.not_found` when interop is absent or the oid is unimportable; skipped DB fails closed `db.not_configured`; malformed env/foreign dir degrades `vcs: None` + warn, never panics; `auth.unauthenticated`→`vcs.unauthenticated`, `email_unverified` passes through. Tests: `oxidean-vcs` 178/178, host VCS suite 13/13, clippy/fmt/check clean. READY.
