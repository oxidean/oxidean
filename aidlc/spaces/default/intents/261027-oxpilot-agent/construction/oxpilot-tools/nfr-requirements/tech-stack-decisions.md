# Tech-Stack Decisions — oxpilot-tools (U5)

Consumes: functional-design questions Q3-B/Q4-A/Q6-A/Q11-A/Q13-A
answers, `codekb/oxidean-app/technology-stack.md`, existing
`crates/oxidean-api-oxpilot` seams (`runtime.rs` emit_frame,
`connector/gateway.rs`), `crates/oxidean-api-core/src/{oxpilot,app,
audit}.rs`, `crates/oxidean-core/src/redact.rs`, `crates/oxidean-db`
verbs, workspace `Cargo.toml` (schemars 1.2 — generation only).

## Decisions

| id | decision | rationale |
|----|----------|-----------|
| TS-1 | Implementation lives inside `crates/oxidean-api-oxpilot` as a tools module — NOT a new crate | `emit_frame` is `pub(crate)` and `invoke(call, ctx)` hands the tool no turn handle — the consent/diff card emit path (`turns().get` → `emit_frame`) only compiles same-crate (BR4.5). The crate already holds runtime + gateway + services; a new crate would force a visibility widening for zero separation gain |
| TS-2 | Read tools dispatch through `state.rpc_dispatch` on a per-call `RpcCtx` synthesized by `build_rpc_ctx_with_session` + `ClientMeta::default()` — never direct `oxidean-db`/`GitBackend`/`vcs` | The shipped seam (app.rs:537) mints a session-equivalent ctx so proc-level authorization (`require_verified`, repo ACL, feature flags, archived checks) applies verbatim (BR3.1). The `oxpilot:`-namespaced synthetic id can't collide with a real session row; no fabricated IP/UA |
| TS-3 | Write-class audit via `audit::record_with(&state.db, &ClientMeta::default(), actor, event, target, detail)` | The ctx-free entry point — tools hold `ToolCtx`, not `RpcCtx`, so `audit::record` doesn't fit; `record_with` already swallows its own failure (BR9.1). Actor resolves via `find_user_by_id` with `None` fallback |
| TS-4 | Redaction via `oxidean_core::redact` — the free function | No `svc.redact` exists; the same function already guards the service boundary and runtime summaries (BR9.5 split: `save_proposal` redacts title/diff_body internally, the tool redacts what it emits — no duplicated redactor) |
| TS-5 | Arg validation is a MINIMAL HAND-ROLLED JSON-Schema check over U5's own closed vocabulary — `type`, `required`, `enum`, `properties`, `items`, `minLength`/`maxLength`, `minItems`/`maxItems`, `minimum`/`maximum`, `pattern`, `additionalProperties: false` | The workspace ships `schemars` 1.2 for schema GENERATION only (oxidean-core, oxidean-api-rest) and has NO runtime-validation crate. The registry authors every `parameters_json` itself, so the validator covers exactly the keyword set the catalog emits — nested `files[]` object arrays (per-element `op` enum + path bounds + closed properties), numeric pagination caps, and `pattern` for path-shape rules, all hand-rollable without a draft-2020-12 engine. Conditional per-op requireds (rename⇒`new_path`, create/update⇒`content_b64`) are NOT schema-expressible and live handler-side as `invalid_args`. Checked workspace + crate manifests: no `jsonschema`/`valico`-style validator exists |
| TS-6 | `serde_json` for `parameters_json`/args plumbing; `base64` for `content_b64` decode via the workspace's existing usage | Already linked everywhere the crate touches; args arrive as `Value` and stay `Value` until per-handler decoding |
| TS-7 | `oxpilot.diff_list`/`diff_get`/`diff_apply` register on the existing `oxpilot.*` PROCEDURES set in `oxidean-api-oxpilot` + dispatch + `make rpc-gen` | Same SessionOnly/`require_enabled`/`require_verified` classification as the shipped oxpilot procs (C1); the additive DB verbs (`find_oxpilot_proposals_by_session`, the `ops_json` chain) ship as a NEW migration ×3 dialects — applied migrations are append-only, 0087 is never edited — plus a `dialect_oxpilot.rs` assertion alongside U2's tables (NFR9 same-change rule) |
| TS-8 | Tests ride the existing in-memory service/test harness: registry-closure pin test (exactly the 23 names, each → its allowlisted proc/kind; mutation vocabulary asserted absent; `diff_apply` asserted entry-less), per-handler tests with stubbed `rpc_dispatch`/`ConnectorGateway`, `diff_apply` negative-path tests | Q13-A — the harness and the stubbed-dispatch pattern already exist; no new test infrastructure, no live provider dependency (NFR9's surface-closure leg for this unit) |
| TS-9 | Zero new dependencies — every seam above already ships in the workspace | Dependency discipline per AGENTS.md: the closed-registry posture extends to the build graph — a tool catalog that can't grow a proc can't grow a crate either |

## Rejected alternatives

- New `oxidean-api-tools` crate — `emit_frame` is `pub(crate)`; a new
  crate forces a visibility change for a seam the design keeps
  internal (TS-1).
- Direct DB/service-verb calls for reads — skips proc-level
  authorization the UI path relies on (components.md pin: "never
  direct DB/git"); rejected at FD Q11-C (TS-2).
- `jsonschema` crate for full-spec validation — a new dependency (a
  complete draft engine) to validate a closed ~12-keyword vocabulary U5
  emits itself; rejected at Q8-B (TS-5).
- Runtime blocklist over the full proc registry — the registry IS the
  allowlist; absence, not filtering, is the posture that can't be
  bypassed (rejected at FD Q5/Q2, NFR1.4).
- Auditing every tool call incl. reads — contradicts the C6 class
  rule verbatim; the durable card row is the read trail (TS-3/NFR5.5).
- `class` on the wire `ToolSpec` — the shipped struct is three-field;
  the class stays internal on `CatalogEntry` (deviation D-01,
  accepted at FD).
