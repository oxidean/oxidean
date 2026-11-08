## Review

**Stage:** code-generation
**Unit:** u7-vcs-api-surface
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| 1 | MINOR | `object_get` buffers the whole object with `read_to_end` — up to `OXIDEAN_VCS_MAX_OBJECT_BYTES` (frozen default 256 MiB) allocated per request. C2 pins `application/octet-stream` + `Cache-Control` (streaming is not pinned) and `release_assets`/`repo_raw` buffer too, so this is hardening rather than contract repair — but a bounded-chunk `Body::from_stream` over `futures_util::stream::unfold` (already imported) + `axum::body::Bytes` removes the per-request allocation with no new deps. | `crates/oxidean-api/src/vcs_routes.rs` `object_get` | Resolved |
| 2 | MINOR | Seven sites emit `AppError::new("internal", …)` — the pinned taxonomy and codebase convention (`app.rs` serialize-failure envelope) use `rpc.internal`. Both map to 500, but the wire `code` diverges from the frozen `rpc.internal` token. | `crates/oxidean-api/src/vcs_routes.rs` `resolve`/`object_get`/`object_put` | Resolved |
| 3 | NIT | `map_auth`'s doc comment claims `auth.unauthenticated`/`auth.email_unverified` both "remap onto the C1 taxonomy"; only `unauthenticated` is mapped — `email_unverified` passes through (intentional: preserves the real reason). Comment overstates the arm. | `crates/oxidean-api/src/vcs.rs` `map_auth` | Resolved |

### Summary

Adversarial pass over the bound bytes: `oxidean-vcs/src/{api,activity}.rs`, `oxidean-api/src/{vcs,vcs_routes,vcs_protection}.rs`, the `rpc.rs` dispatch arms + `RpcCtx` slot, `app.rs` bootstrap/route mount/`build_vcs_slot`, the GC job + factory-reset wiring, `proc_classes` aliases, `main.rs` worker-stack sizing, `.cargo/config.toml`, and the host integration suite.

Verified clean: write-tier `resolve_repo_for_owner_mutate` is capability-based (`Capability::Write` + `ensure_not_archived`) — org/shared repos are covered despite the owner-ish name; `repo_enable` is admin-tier with an explicit `is_skipped` → `db.not_configured` guard that bypasses the flag-row helper correctly; the three-case flag gate is honored on both surfaces (RPC `flag_row`/`require_enabled`; GET row-present vs POST enabled per AC1.2.1); `resolve()` orders Bearer → repo lookup → PAT route-alias authorize → capability with private-repo 404 anti-enumeration before `vcs.forbidden`; `quota_exceeded`→413 typed while `DefaultBodyLimit` 512 MiB sits above the 256 MiB cap; `HostProtectionChecker` consumes `evaluate_push`'s `ProtectionBlockReasons` verdict data and collapses `repo.*`/`admin.*` into a generic `protected` verdict without leaking internals; `lazy()` returns `Ok(())` on absent interop so the retry produces plain `vcs.not_found` per contract (not `vcs.disabled`); `build_vcs_slot` degrades to `None`+warn on malformed env/refused dir/caps refusal and absolutizes the relative objects dir; `err_response` covers the full C1 taxonomy including `ref_conflict`/`conflict`/`no_common_ancestor`/`import_state`→400 and `too_large`→413; `capability_env_label`/`parse_capability` round-trip Admin/Write/read-floor conservatively.

Two MINOR wire-surface defects (bare `internal` code; full-object buffering) and one doc NIT — all mechanical, no design change. NOT-READY pending integration.
