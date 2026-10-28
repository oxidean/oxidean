**Collaborator:** aidlc-developer-agent

## Contribution

Lens: naming, layer boundaries, error handling, file organization, code-style.
Independently inspected: `crates/oxidean-api/src/rpc.rs` + `app.rs`,
`crates/oxidean-api-core/src/rpc.rs`, `crates/oxidean-api-vcs/src/{lib.rs,
dispatch.rs,Cargo.toml}`, `crates/oxidean-api-issues/src/{lib.rs,dispatch.rs}`,
`crates/oxidean-core/src/{proc_classes.rs,vcs_types.rs,lib.rs}`,
`scripts/check-api-domain-graph.mjs`, root + `crates/oxidean-api/Cargo.toml`,
`versioning.toml`, `crates/oxidean-api-vcs/CHANGELOG.md`,
`crates/oxidean-api-rest/src/action_downloads.rs` + `spec.rs`,
`crates/oxidean-web/src/proxy.rs`, `crates/oxidean-db/src/lib.rs`,
`crates/oxidean-api-core/src/templates/handlers.rs` (`db_err`),
`apps/web/src/routes/settings/ai.tsrx` + route tree, `AGENTS.md`,
`docs/CODE_PRACTICES.md`, `docs/ARCHITECTURE.md`,
`.cursor/rules/{oxidean-core,rust-crates,rpc-codegen,use-in-repo-skills}.mdc`,
and affirmed `aidlc/spaces/default/memory/{team.md,project.md}`.

### The rewritten Layers bullet — verified accurate

- `crates/` holds **27** `oxidean-api-*` dirs = **25 domain crates** +
  `oxidean-api-core` + `oxidean-api-rest` (façade, owns no `PROCEDURES`). The
  draft's "25" is right.
- Shell `rpc.rs:15-46`: `PROCEDURES` is a `LazyLock<Vec>` aggregating core's
  residual + all 25 domain lists, then `sort_unstable` + `dedup`. Fan-out
  (`rpc.rs:52-144`) runs `gate` **once**, then first-claim-wins
  `if let Some(...) = oxidean_api_*::dispatch` arms, falling through to
  `dispatch_ungated` — "dispatch fan-out in shell order" holds, and order is
  semantically immaterial because disjointness is machine-enforced.
- Core residual `PROCEDURES` (`oxidean-api-core/src/rpc.rs:173-180`) is
  exactly `system.health/echo/db_probe/manifest`, `invites.accept/get`,
  `search.global` — the draft's "residual `system.*`/`invites.*`/
  `search.global`" is verbatim-correct.
- `check-api-domain-graph.mjs` enforces both claimed invariants: (a) dep
  direction — domain crates may list only `oxidean-api-core` among
  `oxidean-api*` deps, core lists none, the shell may list all
  (`Cargo.toml` scan, L21-47); (b) `PROCEDURES` name-disjointness across
  every domain `lib.rs` + core `rpc.rs` (L49-74). **Precision nit for the
  final text**: the constraint is "only `oxidean-api-core` *among
  `oxidean-api*` crates*" — `oxidean-api-vcs/Cargo.toml` legitimately also
  deps `oxidean-core`, `oxidean-db`, `serde_json`. Worth spelling out so the
  affirmed text isn't misread as "core is the only allowed dep".
- "No `services/` layer; named structs on `AppState`/`RpcCtx`" verified —
  `RpcCtx` (`api-core/src/rpc.rs:87-137`) carries `Arc<dyn EmailSender>`,
  `email_slot: Arc<RwLock<Arc<dyn EmailSender>>>`, `Arc<dyn GitBackend>`,
  `SessionService`, `*Limiter`s, `Option<crate::vcs::SharedVcs>` (None →
  `vcs.disabled` wire error), and `rpc_dispatch: RpcDispatch` — the
  in-process dispatch handle (issue #162) that lets REST façade and edge
  routes (`pages_serve`, `template_packs`, `repo_raw`) call the assembled
  dispatch without depending on the shell.
- `docs/ARCHITECTURE.md:227-230` already documents the split ("shell:
  binaries, route wiring, dispatch assembly" / "`oxidean-api-<dom>`/
  per-domain RPC dispatch crates"). **However the agent-facing rule
  surfaces lag**: `.cursor/rules/rust-crates.mdc:11` still says
  "`oxidean-api`: HTTP/RPC/auth/email", and `docs/CODE_PRACTICES.md:11`'s
  boundary table names only the monolith — neither mentions `api-core`,
  domain crates, or the graph check. If the Layers rewrite is affirmed,
  those two files need matching edits or Construction-stage agents will
  read a pre-#162 boundary description.

### The extended RPC registration checklist — verified, with gaps for a *new* crate

The draft's checklist is accurate for adding a procedure to an existing
crate. Standing up `oxidean-api-oxpilot` itself adds steps the checklist
doesn't name:

- workspace `Cargo.toml` `members` entry (explicit list — every crate is
  enumerated, root `Cargo.toml:5-45`);
- `crates/oxidean-api/Cargo.toml` path dep (L28-54) + shell `rpc.rs`
  `extend_from_slice` + a fan-out arm;
- `versioning.toml` `[[component]]` (`scopes = ["api-oxpilot"]`,
  `depends_on = ["oxidean-api-core"]`, tag_prefix/changelog_path/globs)
  **plus** adding the new id to `oxidean-api`'s `depends_on` (L71) and a
  per-crate `CHANGELOG.md` — corroborated by affirmed corrections:
  registration requires an existing workspace member with literal
  `[package] version` (`version.workspace = true` rejected), and
  `depends_on` edges belong to the unit adding the real Cargo dep;
- crate skeleton follows the domain-crate contract exactly
  (`oxidean-api-issues` is the canonical impl — `oxidean-api-vcs/
  dispatch.rs:1` names it): `lib.rs` = doc comment + `mod dispatch;` +
  handler modules + `pub use dispatch::dispatch;` + `pub const PROCEDURES`;
  `dispatch.rs` = the copy-paste `arm!` macro over
  `match procedure { ... _ => None }` returning `Option<RpcResponse>`;
  handlers return `Result<T, AppError>`.

### `proc_classes` — the draft understates the decision

- Fail-closed confirmed: `classify_pat_proc` doc-comment + signature
  (`proc_classes.rs:6-9,160-161`) — "unlisted procedures classify
  `SessionOnly` — never callable with a token". For a browser-only
  `oxpilot.*` surface that is the desired default, and it should be locked
  with an explicit `assert_eq!(classify_pat_proc(name), SessionOnly)` test
  rather than left as absence.
- But SessionOnly is a **decision**, not a default to drift into: MCP
  (`/api/mcp`, advertised on `settings/ai.tsrx`) and `ox` are
  bearer-credential surfaces that ride the same dispatch — if oxpilot is
  ever meant to be invoked headlessly (agent runs via `ox`/MCP), every
  name needs an explicit non-`SessionOnly` class. The reference test is
  `oxidean-api-vcs/src/dispatch.rs:48-108`: pinned set in
  `oxidean_core::VCS_PROCEDURES` (`vcs_types.rs:26`), asserting
  PROCEDURES↔arms↔classification closed-set **both directions** — the
  reverse-closed-set arm ("an unlisted name would classify SessionOnly yet
  stay invocable by cookie sessions, silently widening the surface") is
  exactly the failure mode `oxpilot.*` should copy.
- Affirmed-correction precedent: repo feature toggles classify
  `RepoAdminWrite` ("the same shape applies to any new repo enable/disable
  RPC") — relevant if oxpilot grows per-repo enablement.

### Naming / file-organization spot checks — all hold

- New-proc `domain.snake_case`: `vcs.*` is uniformly snake
  (`vcs.changeset_create`); old camelCase (`user.getPublicProfile`,
  `repo.listMine`) coexists frozen. → `oxpilot.session_create`, not
  `oxpilot.createSession`.
- Error idiom triple confirmed: thiserror inside libraries; `Database`
  façade returns `Result<T,String>` (`oxidean-db/src/lib.rs:135-186`,
  `// --- domain ---` banners sectioning a 7k-line façade delegating to
  per-domain modules like `pats.rs`/`organizations.rs` — draft's banner
  claim holds); `db_err` adapters match literal text
  (`templates/handlers.rs:17-27` — `e == "database not configured"` →
  `db.not_configured`, else `tracing::error!` + generic
  `admin.internal`). Wire shape `AppError{code,message,data}` + untagged
  `RpcResponse` (`oxidean-core/src/lib.rs:117-138,220-237`). New codes
  should be `oxpilot.<reason>` — and the affirmed correction applies:
  transport denials stay `auth.*`/`rpc.*`, domain codes come from the
  domain layer.
- Web side: `ai.tsrx` confirms kebab-case route file, PascalCase
  component, `@/` alias, `useQuery(authSessionQueryOptions())`,
  `usePageRedirect` auth gate — the `/oxpilot` page should mirror this
  shape + a `src/lib/oxpilot-queries.ts` `queryOptions` factory +
  route-coverage manifest row.
- SSE precedent found: `stream_job_log`
  (`oxidean-api-rest/src/action_downloads.rs:420-478`) — `app.rs`-mounted
  `/api/repos/{owner}/{repo}/actions/jobs/{job_id}/logs/stream`, snapshot
  frame → wake-driven appends → terminal `done` frame, `KeepAlive` 15s.
  It lives in `api-rest` only because it shares download auth helpers —
  it is **not** a `RouteDef`/OpenAPI surface.

## Positions

1. **Affirm the rewritten Layers bullet as team practice, not just current
   fact.** It is verbatim-accurate against the tree (counts, residual list,
   gate-once + fan-out, disjointness machinery). One wording fix: say
   "domain crates may depend on `oxidean-api-core` **only among
   `oxidean-api*` crates**" — foundational deps (`oxidean-core`, `-db`,
   `-git`) are allowed and used. And if affirmed, follow through on the
   stale rule surfaces (`rust-crates.mdc`, `CODE_PRACTICES.md` boundary
   table) in this intent or a chore — they still teach the monolith.

2. **Yes — mandate the domain-crate registry for `oxpilot.*`.** Structural
   phrasing: *a new RPC family lands in a domain crate (new `oxidean-api-*`
   or the existing crate owning that domain) — never in core's residual
   `PROCEDURES`, which is reserved for `system.*`/cross-cutting leftovers.*
   Rationale: every post-#162 domain lives in a crate; core residual is 7
   cross-cutting names, not a feature home; the graph checker auto-scans
   any new `crates/oxidean-api-*` dir so the mandate is free to honor;
   putting `oxpilot.*` in core would re-grow what #162 removed and dodge
   the disjointness enforcement. Nuance: if a genuinely admin-domain proc
   emerges (instance-level provider settings), it belongs in
   `oxidean-api-admin` — cohesion picks the crate, not the intent name.

3. **Extend the checklist explicitly for new-crate registration.** Add:
   workspace `members` entry, shell `Cargo.toml` dep + `rpc.rs` aggregate +
   arm, `versioning.toml` component + `oxidean-api` `depends_on` edge +
   crate `CHANGELOG.md` (per the affirmed literal-version/edge-timing
   corrections), and the per-crate `procedures_list_matches_dispatch` /
   classification tests. The draft's current list reads as
   procedure-in-existing-crate only.

4. **`proc_classes` needs an interview decision, not a "note".** Options:
   (a) deliberate `SessionOnly` for all `oxpilot.*` — correct if the
   surface is browser-session-only; lock with an asserting test;
   (b) explicit class if `ox` CLI / MCP / headless agent invocation is in
   scope (MCP rides bearer auth through the same dispatch — `SessionOnly`
   would silently dead-end that path). Given the intent is an *agentic*
   feature and `settings/ai.tsrx` already sells MCP/`ox` as the
   agent-facing surfaces, I lean (b) for at least the run/status reads —
   but this is a product call. Also recommend adopting the
   `VCS_PROCEDURES`-style pinned const (`OXPILOT_PROCEDURES` in
   `oxidean-core`) + closed-set test — the agent surface is exactly where
   silent wire-surface widening hurts most.

5. **Placement questions for the interview/design (my recommendations):**
   the SSE chat route mounts in `app.rs` under `/api/` (per the affirmed
   `/api/`-prefix correction — `API_PREFIXES` needs no edit) with the
   handler in `oxidean-api-oxpilot`, not `api-rest` (which is the
   GitHub-compat façade, not a general streaming home) and not the shell;
   provider/connector seams follow the `RpcCtx` precedents —
   settings-rebuildable → `Arc<RwLock<Arc<dyn …>>>` `email_slot` shape,
   optional/validated-at-boot → `Option<Shared*>` `vcs` shape with an
   `oxpilot.disabled` wire error — slot *types* live in `api-core` when
   core infra must see them (the `SharedVcs`-in-core precedent), logic in
   the domain crate; agent-internal tool calls ride `RpcCtx.rpc_dispatch`
   rather than duplicating domain logic.
