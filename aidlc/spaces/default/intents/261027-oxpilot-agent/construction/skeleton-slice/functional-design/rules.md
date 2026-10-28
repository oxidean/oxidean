# Business Rules — u1-skeleton-slice

The fenced `yaml` block is the source of truth. Rule IDs use the
`BR{group}.{seq}` format; groups: 1 registration, 2 enablement gate,
3 sessions, 4 stream, 5 web surface. Rule sources cite
`inception/requirements-analysis/requirements.md` FR/NFR ids and
`inception/user-stories/stories.md` AC ids.

## Rule register

```yaml
rules:
  - id: BR1.1
    statement: OXPILOT_PROCEDURES names exactly {oxpilot.status, oxpilot.session_create, oxpilot.session_list}
    category: constraint
    applies_to: oxidean-api-oxpilot crate
    trigger: compile/gate test
    logic: IF the exported set differs in member or count THEN the closed-set test fails
    violation: build fails — no silent proc addition
    source: FR6.2, AC5.5.1
  - id: BR1.2
    statement: Every oxpilot.* proc is SessionOnly — achieved by absence from proc_classes PAT tiers; a closed-set test asserts classify(each oxpilot.*) == SessionOnly and fails if any oxpilot.* name enters a PAT tier
    category: authorization
    applies_to: proc_classes + PAT path
    trigger: every dispatch; pinned by test
    logic: IF credential is PAT/job-token AND proc matches oxpilot.* THEN deny before handler
    violation: PAT refusal test fails; dispatch denies at runtime
    source: FR6.2, NFR1, AC5.5.1
  - id: BR1.3
    statement: Registration parity — PROCEDURES entry, dispatch arm, proc_classes decision, versioning.toml component, workspace member, and CHANGELOG land in the same change
    category: constraint
    applies_to: crate registration
    trigger: review/CI
    logic: IF any registration surface is missing THEN procedures_list_matches_dispatch and versioning checks fail
    violation: CI failure
    source: FR6.2, AC5.5.1
  - id: BR1.4
    statement: New oxpilot tables ship dialect_oxpilot.rs inside the db-dialects -E filter and a TESTING.md row in the same change
    category: constraint
    applies_to: migrations
    trigger: CI dialect lane
    logic: IF oxpilot tables exist without dialect coverage THEN dialect lane fails
    violation: CI failure
    source: FR6.6, AC5.5.3
  - id: BR1.5
    statement: Every new OXIDEAN_OXPILOT_* env var lands in .env.example AND docs/CONFIGURATION.md in the same change
    category: constraint
    applies_to: env surface + docs
    trigger: review/CI docs check
    logic: IF an OXPILOT env var exists that docs don't name THEN the change is incomplete
    violation: review finding; docs parity check
    source: FR6.4, AC5.1.4

  - id: BR2.1
    statement: enabled = flag_on AND provider slot non-None; flag_on = env OXIDEAN_OXPILOT_ENABLED (default OFF) unless an oxpilot_settings.enabled override row exists
    category: policy
    applies_to: EnablementState
    trigger: every oxpilot.* call and SSE route hit
    logic: IF flag_on AND provider present THEN enabled ELSE disabled; reason precedence flag_off > misconfigured > no_provider
    violation: deny (see BR2.3)
    source: FR6.4, FR6.5, AC5.1.5
  - id: BR2.2
    statement: OXIDEAN_OXPILOT_ENABLED defaults OFF when unset (new agentic spend feature fails closed, unlike MCP's default-on)
    category: policy
    applies_to: EnablementState.flag_on
    trigger: config read
    logic: IF env unset THEN flag_on=false
    violation: n/a — definition
    source: FR6.4, US5.5 fail-closed
  - id: BR2.3
    statement: oxpilot.status is gate-exempt — it answers {enabled, provider, reason} whether enabled or not; every other oxpilot.* proc and both SSE routes deny oxpilot.disabled server-side
    category: authorization
    applies_to: all gated surfaces
    trigger: any gated call while disabled
    logic: IF NOT enabled AND proc != oxpilot.status THEN deny oxpilot.disabled
    violation: typed domain error; UI renders flag-off state from status.reason
    source: AC5.1.5, AC5.1.1
  - id: BR2.4
    statement: Env/DB override follows admin.mcp.* semantics — DB override row wins over env; absent row → env value
    category: policy
    applies_to: EnablementState.flag_on
    trigger: each resolution (no cross-request caching past a settings write)
    logic: IF override row present THEN use it ELSE env default
    violation: n/a — definition; U4 ships the override row, U1 ships env + seam
    source: FR6.5, AC5.1.3
  - id: BR2.5
    statement: Provider slot resolution — OXIDEAN_OXPILOT_PROVIDER=stub installs StubProvider (model = OXIDEAN_OXPILOT_MODEL, default stub-1); unset ⇒ slot None; any other value ⇒ slot None + reason misconfigured
    category: policy
    applies_to: ProviderHandle, EnablementState.reason
    trigger: slot resolution at startup + each gated call
    logic: match(provider_env) { "stub" => StubProvider, unset => None(no_provider), _ => None(misconfigured) }; U4 teaches the registry the real adapters
    violation: denied calls carry the reason; misconfigured is observable in oxpilot.status
    source: FR6.4, AC5.1.5

  - id: BR3.1
    statement: session_create binds caller user_id + validated repo + branch; repo must resolve via existing repo rules (visibility, existence) — invalid repo ⇒ not_found, never a session on a foreign/absent repo
    category: validation
    applies_to: oxpilot.session_create
    trigger: create call
    logic: IF repo unresolvable for caller THEN not_found ELSE insert row with repo_id, branch, title (goal-derived), status=active
    violation: not_found error; no row
    source: FR1.3, AC1.2.2
  - id: BR3.2
    statement: session_list returns only the caller's rows, newest first by last_activity_at
    category: authorization
    applies_to: oxpilot.session_list
    trigger: list call
    logic: SELECT WHERE user_id = caller ORDER BY last_activity_at DESC
    violation: impossible by construction (filter is mandatory)
    source: AC1.1.1, AC5.5.4
  - id: BR3.3
    statement: Cross-user access denies with not_found (anti-enumeration), never revealing the row exists — applies to session_get/chat/stream/delete paths
    category: authorization
    applies_to: all session-scoped reads/writes
    trigger: any session-scoped call
    logic: IF row.user_id != caller THEN not_found (identical to absent row)
    violation: not_found response; audit optional (security-relevant → fire-and-forget)
    source: AC5.5.4, NFR4
  - id: BR3.4
    statement: Every accepted user message and assistant reply persists as OxpilotMessage rows before/with streaming — transcript survives reopen
    category: constraint
    applies_to: oxpilot_messages
    trigger: chat POST / stream completion
    logic: user row appended before provider call; assistant row assembled as stream runs, marked complete at terminal{done}, error/aborted otherwise
    violation: partial rows keep status≠complete and render as such on resume
    source: AC1.3.3, FR2.4
  - id: BR3.5
    statement: Message content is secrets-redacted on write (NFR1) — the stored transcript never holds detected secret material
    category: constraint
    applies_to: OxpilotMessage.content
    trigger: every message write
    logic: redact(content) THEN insert
    violation: n/a — unconditional transform
    source: NFR1, AC5.3.3
  - id: BR3.6
    statement: client_msg_id is the caller-supplied idempotency key — a chat POST whose client_msg_id already exists on the session re-attaches to the existing turn instead of inserting a duplicate user row
    category: constraint
    applies_to: chat POST route, oxpilot_messages
    trigger: POST with client_msg_id
    logic: IF (session_id, client_msg_id) row exists THEN — turn still live ⇒ attach to its stream; turn already ended ⇒ answer with the persisted transcript (snapshot-equivalent, no new turn, no new row) ELSE normal accept path
    violation: unique (session_id, client_msg_id) — belt-and-suspenders only; an in-flight raced duplicate exits earlier at the ActiveTurn claim (409)
    source: contract-design Q4 (idempotent resend), C2 rules

  - id: BR4.1
    statement: POST /api/oxpilot/sessions/{id}/chat requires session auth + ownership + enabled gate + valid body, else typed JSON error — never a partial stream
    category: validation
    applies_to: chat POST route
    trigger: request
    logic: checks order: auth → gate → ownership → body; first failure wins with its typed code
    violation: 401/403 oxpilot.disabled/404/422 JSON body
    source: AC5.1.5, AC5.5.4, C2 rules
  - id: BR4.2
    statement: A second chat POST while a turn streams on that session ⇒ 409 turn_active — the check is the atomic insert-if-absent into the ActiveTurn map, never a check-then-act (no auto-enqueue in the skeleton — message_queue lands in U2)
    category: constraint
    applies_to: chat POST route, ActiveTurn
    trigger: POST during live turn
    logic: IF ActiveTurn.insert(session) fails THEN 409 turn_active; the winner proceeds under BR4.6
    violation: typed JSON error; nothing queued
    source: C2 rules, US1.8 groundwork
  - id: BR4.3
    statement: Stream frames follow the pinned order — assistant message opens, append deltas (per-message counter monotonic; the frame's wire field is `seq` per C2 — `delta_seq` is only the design-internal name for that same field, distinct from the session-level message seq), exactly one terminal frame last (done|error|aborted); ping keep-alive ≥ every 30 s
    category: constraint
    applies_to: SSE emission
    trigger: every stream
    logic: frames emit in {append*, terminal} order; no frame after terminal
    violation: protocol violation — client drops connection
    source: AC1.3.1, C2 frames
  - id: BR4.4
    statement: GET .../chat/stream emits a snapshot frame (all persisted messages + active_turn from the ActiveTurn map or null + pending_queue — always present, empty in U1) before any live append — resume is snapshot-replace then append, no Last-Event-ID
    category: constraint
    applies_to: stream GET route
    trigger: GET request
    logic: emit snapshot{messages, active_turn{turn_id, started_seq, message_id, last_delta_seq}|null, pending_queue} THEN buffered-then-live frames; a mid-turn attach replays the turn's buffered appends and the client drops any with delta_seq ≤ the snapshot's last_delta_seq (the in-flight row's persisted content is the base) — no live turn ⇒ stream stays open on ping cadence for later appends; client replaces state on snapshot, dedupes appends by (message_id, seq)
    violation: client refetches snapshot
    source: AC1.3.1, C2 rules
  - id: BR4.5
    statement: The stub provider streams a deterministic canned reply (tokenized over frames) — no network, no key, deterministic output for tests
    category: policy
    applies_to: ProviderHandle (stub impl)
    trigger: chat POST while enabled
    logic: reply = f(echo of goal + last user message) split into fixed deltas
    violation: n/a
    source: U1 boundary (skeleton), Q4
  - id: BR4.6
    statement: ActiveTurn lifecycle — per-process concurrent map keyed session_id owned by RpcCtx; insert-if-absent claims the turn (BR4.2) and mints a fresh turn_id; the entry carries the frame fan-out channel + delta buffer; the turn task owns the assistant row to terminal status and the entry is removed when the task finishes; transport close only detaches readers (detach-to-complete); cancel fires on explicit abort/delete-hook/shutdown; resumable GETs attach readers but never own it
    category: constraint
    applies_to: ActiveTurn (entities.md), chat POST + stream GET
    trigger: turn claim/terminal/disconnect
    logic: claim⇒insert{fresh turn_id, started_seq=next message seq, pre-minted assistant message_id, fanout, buffer} — the entry is fully-formed at claim so a mid-window snapshot is conformant; POST publishes frames to fanout; GET subscribes (buffered replay then live); release⇒remove when the turn task ends; orphans swept to error at startup
    violation: a leaked entry pins the session at 409 — task-lifetime release + startup sweep prevent it
    source: R-03 finding fix (turn-state ownership), C2 rules

  - id: BR5.1
    statement: /oxpilot route registration is the full set — SHELLS entry, astro shell page, .tsrx route, RESERVED_TOP_SEGMENTS += "oxpilot", regenerated routeTree, coverage-manifest rows for index+workspace
    category: constraint
    applies_to: apps/web
    trigger: route-coverage CI lane
    logic: IF any registration surface missing THEN coverage/manifest checks fail
    violation: CI failure
    source: AC1.1.3, FR1.1
  - id: BR5.2
    statement: /oxpilot admits any authenticated user — no role gate in v1; unauthenticated visitors get the standard auth redirect
    category: authorization
    applies_to: web routes + RPC
    trigger: request
    logic: IF no session cookie THEN login redirect/401 ELSE admit
    violation: redirect/401
    source: AC1.1.5, FR1.6
  - id: BR5.3
    statement: The skeleton page is intentionally minimal — session list, new-session form (repo, branch, goal), transcript, composer; rail/drawer/cards/aria-live choreography are U8 scope and their absence is not a defect here
    category: constraint
    applies_to: web surface
    trigger: review
    logic: skeleton proves the round trip; full surfaces land in U8
    violation: scope creep finding at review
    source: Q5, US1.3 skeleton note
  - id: BR5.4
    statement: Flag-off page state names the OXIDEAN_OXPILOT_* vars and points at docs/CONFIGURATION.md — explanatory, never a bare error
    category: policy
    applies_to: web surface + oxpilot.status.reason
    trigger: enabled=false page load
    logic: render disable state naming OXIDEAN_OXPILOT_ENABLED + provider vars + docs link
    violation: UX finding at review
    source: AC5.1.1, FR1.5
```

## Rules summary

| id | one-line |
|----|----------|
| BR1.1 | exactly 3 pinned proc names |
| BR1.2 | oxpilot.* = SessionOnly by omission + closed-set test |
| BR1.3 | registration parity across 6 surfaces in one change |
| BR1.4 | dialect_oxpilot.rs + filter + TESTING.md together |
| BR1.5 | every OXPILOT_* env var documented in .env.example + CONFIGURATION.md |
| BR2.1 | enabled = flag AND provider; reason precedence flag_off>misconfigured>no_provider |
| BR2.2 | env default OFF |
| BR2.3 | only `status` answers while disabled |
| BR2.4 | DB override beats env (admin.mcp.* pattern) |
| BR2.5 | provider slot = f(OXIDEAN_OXPILOT_PROVIDER): stub|unset|other |
| BR3.1 | create binds caller+repo+branch, validated |
| BR3.2 | list is caller-scoped, newest-first |
| BR3.3 | cross-user ⇒ not_found anti-enumeration |
| BR3.4 | every message persists, status-marked |
| BR3.5 | content redacted on write |
| BR3.6 | client_msg_id idempotent resend ⇒ re-attach, no dup |
| BR4.1 | POST gates: auth→flag→owner→body, typed JSON errors |
| BR4.2 | concurrent POST ⇒ 409 turn_active via atomic insert-if-absent |
| BR4.3 | append*→terminal ordering, delta_seq monotonic |
| BR4.4 | GET stream = snapshot(+active_turn,pending_queue) then live appends |
| BR4.5 | stub provider = deterministic canned stream |
| BR4.6 | ActiveTurn claimed at POST, released when the turn task ends (detach-to-complete) |
| BR5.1 | full route-registration set in one change |
| BR5.2 | any authenticated user, no role gate |
| BR5.3 | skeleton page minimal by design |
| BR5.4 | flag-off state is explanatory |
