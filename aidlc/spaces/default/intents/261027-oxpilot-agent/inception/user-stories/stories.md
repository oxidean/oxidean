# User Stories — `/oxpilot`

Format: INVEST; IDs `US{group}.{seq}`; acceptance criteria `AC{g}.{s}.{n}`;
MoSCoW mirrors the scope document. Groups: US1 sessions/chat, US2 builder,
US3 debugger, US4 computer use, US5 operator/platform. Integrated after the
mob round (design + developer + quality contributions applied).

## US1 — Sessions & chat

### US1.1 — Browse my sessions [Must]
As Dana, I want `/oxpilot` to list my sessions (title, repo, status, last
activity), so that I can pick up where I left off.
- AC1.1.1 Index renders newest-first session list for the signed-in user only.
- AC1.1.2 Each row shows session title, repository, branch, status, and
  relative time.
- AC1.1.3 Route registered per FR1.1 (SHELLS + shell + route + manifest +
  RESERVED_TOP_SEGMENTS + routeTree).
- AC1.1.4 With zero sessions the index shows an explanatory empty state
  with the new-session entry point.
- AC1.1.5 The route admits any authenticated user — no additional role
  gate for real providers (FR1.6). Post-build: under the resolved `stub`
  provider the route is sys-admin-only (`restricted` ⇒ not-found for
  everyone else).

### US1.2 — Start a session [Must]
As Dana, I want a new-session dialog (repository, branch, goal, optional
computer-use target), so that the agent knows what I'm working on.
- AC1.2.1 Dialog offers repo select, branch select, goal textarea, optional
  target select; Cancel + Start session actions.
- AC1.2.2 Starting creates a persisted session bound to repo + branch.
- AC1.2.3 Dialog is labelled, focus-trapped, and dismissable with Esc.

### US1.3 — Chat with the agent [Must]
As Dana, I want streamed multi-turn chat against my repo, so that I get
repo-aware help without leaving the forge.
- AC1.3.1 Replies stream over SSE (dedicated `/api/` route, snapshot-then-
  append) without page reload.
- AC1.3.2 The workspace shows rail + transcript + toggled context drawer.
- AC1.3.3 Transcript persists server-side; reopening a session restores it.
- AC1.3.4 `aria-live="polite"` announces message boundaries; composer is a
  labelled form; controls are real buttons.
- AC1.3.5 The session binds repo context through `oxpilot.*`-served read
  tools covering file read, tree listing, branch info, commit history, CI
  status, and issue/PR read (FR7.1, FR2.3).
- AC1.3.6 Agent tool calls render as visible in-transcript cards (tool
  name + summarized inputs), so the user can see what the agent did.

**Skeleton note:** US1.3 is the walking-skeleton slice — a thin chat
round-trip (page → `oxpilot.*` RPC → stubbed provider → streamed reply)
lands first; construction splits the remainder along tool-surface lines.

### US1.4 — Handle provider failures [Must]
As Dana, I want a visible terminal error when the provider fails —
including mid-stream — so that I know to retry or report.
- AC1.4.1 A failed stream emits a terminal error frame rendered as an
  in-session error state (provable, not "never a silent stall").
- AC1.4.2 Session and transcript are preserved after the error; the
  composer remains usable.

### US1.5 — Delete a session [Must]
As Dana, I want to delete my session and its transcript behind a confirm,
so that I control my stored data.
- AC1.5.1 Delete requires an explicit confirmation dialog.
- AC1.5.2 Delete removes the session, transcript, tool-call, and
  per-session-grant rows for that user only.
- AC1.5.3 Deleted sessions vanish from the index.

### US1.6 — Resume after navigation [Should]
As Dana, I want my streamed reply to resume cleanly after navigating away
or reconnecting, so that I don't lose or duplicate output.
- AC1.6.1 Reconnect re-requests the stream and applies the snapshot-
  replace + append contract (full snapshot replaces state; subsequent
  frames append) — no `Last-Event-ID` exists; the contract is snapshot-
  based, asserted exactly-once per message.

### US1.7 — Manage computer-use targets [Must]
As Dana, I want to manage my connected targets from the `/oxpilot` index,
so that target lifecycle lives next to the sessions that use it.
- AC1.7.1 Index hosts a targets section: list with online/offline status,
  connect, disconnect, revoke.
- AC1.7.2 `/settings/ai` links into this section; `/settings/ai` remains
  the external-agent integration surface.
- AC1.7.3 Empty targets state explains what a connector is and where to
  start.

### US1.8 — Queue a follow-up while the agent works [Should]
As Dana, I want to type and queue a follow-up while a reply is streaming,
so that I don't wait idle.
- AC1.8.1 Sending mid-stream queues the message; it is delivered after the
  current reply ends.
- AC1.8.2 The queued message is visible in the transcript as pending.

## US2 — Builder

### US2.1 — Get a proposed diff [Must]
As Dana, I want the agent to propose a reviewable diff for my repo, so
that I can inspect exactly what it would change.
- AC2.1.1 In-session diff/patch view shows per-file changes.
- AC2.1.2 The reply links the produced diff artifact (asserted as artifact
  linkage in the transcript, not graded prose).

### US2.2 — Materialize branch + PR [Must]
As Dana, I want to turn an approved proposal into a branch + PR, so that it
enters normal review.
- AC2.2.1 Explicit user action creates branch + PR via existing RPC.
- AC2.2.2 Session links the created PR.
- AC2.2.3 The agent never pushes/merges without that explicit action.
- AC2.2.4 The created PR body discloses that the change was produced by an
  `/oxpilot` agent session (with the session reference) — Riley sees
  provenance as a normal PR. Deliberate carve-out: this is a
  product-transparency requirement for agent-authored PRs, not the banned
  attribution on our own development commits/PRs (the no-AI-attribution
  rule governs repo tooling metadata, not a shipped product feature).

### US2.3 — See a plain patch [Should]
As Dana, I want a raw patch view of the proposal, so that I can apply it
elsewhere myself.
- AC2.3.1 "Show patch" renders the raw unified diff.

## US3 — Debugger

### US3.1 — Inspect a failing run [Must]
As Dana, I want the agent to list Actions runs/jobs and pull logs, so that
I can ask why CI failed without leaving the chat.
- AC3.1.1 Agent lists runs/jobs for the bound repo (read-only).
- AC3.1.2 The reply links the run/job/log artifacts it inspected (artifact
  linkage, not graded summary).
- AC3.1.3 No rerun/cancel/approve control is exposed.

### US3.2 — Propose a CI fix [Should]
As Dana, I want the agent to propose a fix for the failure via the same
reviewable path, so that fixing CI is one loop.
- AC3.2.1 Fix proposals flow through the US2.1/US2.2 review path.

## US4 — Computer use

### US4.1 — Register and pair a target [Must]
As Casey, I want to run a first-party connector on my own system and pair
it to my account, so that the agent can reach my machine.
- AC4.1.1 The connector dials out to the forge; no inbound listener needed.
- AC4.1.2 Pairing follows a short-lived, user-confirmable code/claim flow
  (device-grant-style) producing an auditable target record bound to my
  user.
- AC4.1.3 Target secret material is AES-256-GCM at rest / hashed verifier —
  never returned by any agent-reachable surface.
- AC4.1.4 The pairing code is single-use, short-lived, shown once in the
  UI, and never appears in transcripts or audit detail.

### US4.2 — Consent to a connection [Must]
As Casey, I want an explicit connect-consent dialog naming exactly what the
target grants, so that nothing connects without my informed approval.
- AC4.2.1 The dialog names the granted capabilities (shell exec, fs
  read/write — no GUI) and is labelled + focus-trapped.
- AC4.2.2 Declining leaves the target unconnected; a denied grant is
  auditable.
- AC4.2.3 Connecting without consent is impossible — the target state is
  server-enforced, not UI-enforced.

### US4.3 — Agent acts on my system [Must]
As Casey, I want the agent to run commands and read/write files on my
connected target, so that it can verify or fix things on my machine.
- AC4.3.1 Command exec + fs read/write tools work only against a connected
  target with a live grant.
- AC4.3.2 Grants are per-action or per-session; no cross-session "always
  allow" in v1 (per-action grants render as an in-transcript consent card —
  Screen 4).
- AC4.3.3 Every action emits an audit row (actor, action, time, result)
  with secrets-redacted detail.
- AC4.3.4 An offline/disconnected target produces a visible in-session
  error, not a silent drop.
- AC4.3.5 A denied action grant leaves the action unrun and tells the user.

### US4.4 — Revoke a target [Must]
As Casey, I want to disconnect/revoke a target behind a confirm, so that
access ends the moment I say so.
- AC4.4.1 Revoke requires an explicit confirmation.
- AC4.4.2 Revoke terminates pending grants immediately — including grants
  covering an in-flight action.
- AC4.4.3 Revoked targets refuse new actions.

### US4.5 — See connection status [Should]
As Casey, I want the session rail to show each target's online/offline
state, so that I know what's reachable.
- AC4.5.1 Rail shows target name + status; rail is collapsible.

### US4.6 — Connector client + dial-out protocol [Must]
As Casey, I want a small, documented connector I can run locally or on a
remote host, so that connectivity works without inbound firewall changes.
- AC4.6.1 Connector authenticates via the pairing flow (AC4.1.2), maintains
  an outbound channel, and exposes only the exec/fs surface (FR5.3).
- AC4.6.2 Connector auth is PAT-class with hashed verifier material;
  reconnect/backoff behavior is documented.
- AC4.6.3 The protocol surface is covered by outbound-HTTP fixture tests.

## US5 — Operator & platform

### US5.1 — Enable and configure the feature [Must]
As Quinn, I want `OXIDEAN_OXPILOT_*` env vars + an admin settings override
to enable the feature and pick a provider, so that I control rollout and
cost.
- AC5.1.1 Flag off / no provider → explanatory disable state naming the env
  vars and `docs/CONFIGURATION.md`.
- AC5.1.2 Providers: OpenAI, Anthropic, Ollama, OpenAI-compatible
  (base-url required, key optional); key/base-url env vars; `stub`
  resolves by default and is sys-admin-only.
- AC5.1.3 Flag follows the `admin.mcp.*` env-default + DB-override pattern.
- AC5.1.4 Every new env var is documented in `.env.example` + CONFIGURATION.
- AC5.1.5 With the flag off or no provider configured, `oxpilot.*` RPC
  calls and the SSE route deny with a domain wire error
  (`oxpilot.disabled`) — the denial is server-enforced, not UI-only.

### US5.2 — Control spend [Must]
As Quinn, I want per-user and per-instance message/token budgets, so that
LLM spend stays bounded.
- AC5.2.1 Budgets configurable via env; defaults generous.
- AC5.2.2 Over-budget requests get a clear in-session refusal.

### US5.3 — Audit agent actions [Must]
As Quinn, I want connector actions and approvals in `audit_events`, so
that I can answer "who did what."
- AC5.3.1 Connect/consent/act/revoke lifecycle rows exist.
- AC5.3.2 Audit detail never contains secret material.
- AC5.3.3 Stored transcript content is secrets-redacted on write (NFR1).

### US5.4 — Self-host without a cloud key [Should]
As Quinn, I want Ollama as a provider option, so that `/oxpilot` works
fully local.
- AC5.4.1 Ollama adapter is selectable via env config.

### US5.5 — Platform registration rules [Must]
As Quinn, I want the feature's procedures, data, and secrets handled under
the affirmed platform rules, so that the surface stays fail-closed.
- AC5.5.1 `oxpilot.*` procs live in `oxidean-api-oxpilot` and are
  `SessionOnly`-classified — a pinned closed-set test proves PAT refusal.
- AC5.5.2 Agent write/comment tools are scoped to artifacts the agent
  created with user approval (FR7.5).
- AC5.5.3 New `oxidean-db` tables ship `dialect_oxpilot.rs` in the
  `db-dialects` `-E` filter + TESTING.md table in the same change.
- AC5.5.4 Cross-user access denies (negative-path): a non-owner cannot
  fetch, stream, or delete another user's session or transcript, and
  cannot connect, act on, or revoke another user's target.

## Dependencies

- US1.3 blocks US2.x, US3.x, US4.3 (all ride the session).
- US4.1 → US4.2 → US4.3 → US4.4 (consent before action; revocation has
  meaning only after action exists).
- US5.1 blocks everything (flag/provider gate the feature).
- US4.6 supplies the client side of US4.1–US4.5.

## INVEST notes

Each story is independently shippable behind the feature flag (US5.x and
US1.x are the only hard ordering dependencies); acceptance criteria are
testable at HTTP/RPC or UI level — LLM output is never the assertion (only
artifact linkage and observable state); Won't-tier items are absent —
everything out-of-v1 lives in requirements.md Out of Scope.
