# Phase 17: Notifications - Research

**Researched:** 2026-09-16
**Domain:** In-app activity notifications (issue + PR) on Oxidean Rust RPC + Octane chrome + multi-dialect DB
**Confidence:** HIGH (codebase patterns / issue hooks) / MEDIUM (PR hooks — Phase 12 domain not on this branch yet)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Emit in-app notifications for **issue** activity: opened, closed, reopened, commented, assigned, unassigned — **Reversibility:** costly — event hooks in issue write paths
- **D-02:** Emit for **PR** activity per Phase 12 CONTEXT: opened, closed, reopened, merged, review submitted (Approve / Request changes / Comment), general Conversation comments, line comments, **requested as reviewer** — **Reversibility:** costly — depends on Phase 12 PR domain call sites
- **D-03:** Recipients follow GitHub participant rules: **author**, **assignees**, **prior commenters/participants**, **@mentioned** users (markdown `@username`), **requested reviewers**; **never notify the actor** — **Reversibility:** reversible
- **D-04:** Skip noisy/low-signal events in Phase 17: emoji reactions, label-only changes, force-push / retarget / draft toggles, merge-strategy settings — **Reversibility:** reversible
- **D-05:** Subject links use stable forge URLs: `/{owner}/{repo}/issues/{n}` and `/{owner}/{repo}/pull/{n}` (align Phase 12 URL discretion) — **Reversibility:** reversible
- **D-06:** **In-app only** for Phase 17 — do **not** send EmailSender mail for activity notifications (adapters are for auth/org invites; email notifs need prefs/templates → defer) — **Reversibility:** reversible (deferral)
- **D-07:** No per-user notification preference UI in this phase — defaults above are fixed — **Reversibility:** reversible
- **D-08:** Signed-in **`SiteHeader`** shows a **bell** control with **unread count badge**; links to `/notifications` — **Reversibility:** reversible
- **D-09:** `/notifications` page: filters **Unread | All**; sort **newest first**; **offset pagination** (match Issues/PRs list patterns) — **Reversibility:** reversible
- **D-10:** Each row shows reason text + repo + subject title/`#N`; activating a row **marks that notification read** and **navigates** to the subject — **Reversibility:** reversible
- **D-11:** Optional compact dropdown preview from the bell is **Claude's Discretion** (page is mandatory; dropdown if cheap without duplicating list logic badly) — **Reversibility:** reversible
- **D-12:** Persist `read_at` (null = unread); RPC: **list** (filter + pagination), **markRead** (one or many ids), **markAllRead**, **unreadCount** for the badge — **Reversibility:** costly — schema + RPC surface
- **D-13:** NOTF-02 satisfied by list + mark read (+ mark all); no delete/archive/Done inbox in this phase — **Reversibility:** reversible
- **D-14:** Unread badge uses **TanStack Query** poll/refetch (chrome session pattern); **no websockets** in Phase 17 — **Reversibility:** reversible
- **D-15:** Notification RPCs require signed-in cookie session; users only see/mutate **their own** rows — **Reversibility:** reversible

### Claude's Discretion
- Exact RPC namespace (`notification.*` vs `notif.*`) and migration number after packages/releases
- Notification row payload shape (reason enum vs free-text reason_key + params)
- Whether bell opens a small dropdown preview or only navigates to `/notifications`
- Poll/staleTime interval for unreadCount
- How @mention parsing shares issue/PR markdown pipeline
- Whether mark-read-on-navigate is optimistic UI

### Deferred Ideas (OUT OF SCOPE)
- Email / digest notifications and preference center
- Repo watch / unwatch / custom subscription matrix beyond GitHub-default participant rules
- Realtime websockets / SSE push
- Actions / packages / release / LFS activity notifications
- Notification "Done" archive / unsubscribe threads (beyond mark read)
- Formal shared event bus with Phase 18 webhooks
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| NOTF-01 | Signed-in user receives in-app notifications for relevant issue and PR activity | `notifications` table + fan-out from issue/PR write RPCs; participant + mention recipient rules; never notify actor |
| NOTF-02 | User can list and mark notifications as read | `notification.list` / `markRead` / `markAllRead` / `unreadCount`; `/notifications` UI + header badge |
</phase_requirements>

## Summary

Phase 17 adds a per-user in-app notification inbox for issue and PR activity. No new frameworks or npm packages are required: extend `oxidean-db` with a `notifications` migration, add a `notification` API module dispatched from `rpc.rs`, regenerate `@oxidean/api-client` via `make rpc-gen`, hook fan-out after successful issue (and later PR) mutations, and expose a signed-in bell + `/notifications` page in Octane using TanStack Query.

EmailSender already exists for auth/org mail `[VERIFIED: crates/oxidean-api/src/email/mod.rs:36-39]` quote: `pub trait EmailSender: Send + Sync { async fn send(&self, msg: OutboundEmail) -> Result<(), EmailError>; }` — but activity email needs preference UX and templates (locked out by D-06). Phase 12 PR handlers are not present on this worktree yet (`crates/oxidean-api/src/issue/` exists; no `pull`/`pr` module) `[VERIFIED: directory listing this session]` — plan PR hooks with an execute-time precondition that Phase 12 has landed.

**Primary recommendation:** Ship `notification.*` RPCs + `notifications` table with `read_at`, a shared `notify::fanout` helper used from issue (and PR) write paths, server-side `@username` mention extraction via `find_user_by_username`, header bell with Query-polled `unreadCount`, and `/notifications` list matching Issues offset pagination.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Persist notification rows | Database / Storage | API / Backend | Authoritative inbox per recipient |
| Fan-out on issue/PR writes | API / Backend | — | Same transaction/request as mutation; ACL already enforced |
| List / mark read / unreadCount RPC | API / Backend | — | Session-scoped; own rows only (D-15) |
| @mention recipient resolution | API / Backend | Database / Storage | Parse body text; resolve via `find_user_by_username` |
| Header bell + badge | Browser / Client | Frontend Server (SSR optional) | Query poll; signed-in chrome only |
| `/notifications` page | Browser / Client | API / Backend | List + mark read UX |
| Subject deep links | Browser / Client | — | Client builds `/{owner}/{repo}/issues\|pull/{n}` from RPC fields |

## Project Constraints (from `.cursor/rules/`)

| Rule | Directive |
|------|-----------|
| `oxidean-core.mdc` | One product; Bun + Cargo; Octane `.tsrx` not React; `make rpc-gen`; dialect SQL only in `oxidean-db`; no secrets; prefer existing patterns |
| `octane-ui.mdc` | `.tsrx` with `@{` / `@if`/`@else` (no `@else if`) / `@for`; TanStack Query via session helpers |
| `rpc-codegen.mdc` | Rust types authoritative; regenerate api-client; `make rpc-sync-check` clean |
| `rust-crates.mdc` | core = types; db = SQL; api = handlers; preserve auth gates |

## Standard Stack

### Core

| Library / Component | Version | Purpose | Why Standard |
|---------------------|---------|---------|--------------|
| Axum RPC + `oxidean-api` | in-repo | `notification.*` procedures | Existing dispatch `[VERIFIED: crates/oxidean-api/src/rpc.rs:71]` quote: `pub async fn dispatch(ctx: &mut RpcCtx, req: RpcRequest) -> RpcResponse {` |
| `require_verified` | in-repo | Gate mutating issue paths that emit events | `[VERIFIED: crates/oxidean-api/src/auth/gate.rs:23-28]` quote: `pub async fn require_verified(ctx: &RpcCtx) -> Result<UserRow, AppError> {` / `auth.unauthenticated` |
| `oxidean-db` migrations | next free `00NN` after `0015_packages` (or after Phase 12 PR migration if present at execute) | `notifications` schema | Latest on this branch `[VERIFIED: crates/oxidean-db/migrations/postgres/0015_packages.sql exists]` |
| Octane `.tsrx` + `@octanejs/tanstack-query` | in-repo | Bell + list UI | Project UI stack `[VERIFIED: AGENTS.md]` |
| `SiteHeader` / `AccountActions` | in-repo | Bell insertion beside Create/Account menus | `[VERIFIED: apps/web/src/components/chrome.tsrx:512]` / `[VERIFIED: apps/web/src/components/chrome.tsrx:260-269]` signed-in cluster with `CreateMenu` + `AccountMenu` |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| Lucide icons (existing) | in-repo | Bell icon | Match chrome iconography |
| `authSessionQueryOptions` | in-repo | Pattern for soft Query RPCs | Mirror for `unreadCount` `[VERIFIED: apps/web/src/lib/session-queries.ts:18-34]` |
| `find_user_by_username` | in-repo | Resolve @mentions | `[VERIFIED: crates/oxidean-db/src/lib.rs:802]` quote: `pub async fn find_user_by_username(&self, username: &str)` |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Per-user notification rows | Live query over activity log | Harder pagination/mark-read; GitHub-style inbox wants materialised rows — **prefer rows** |
| Websockets unread push | Query poll | Extra infra; D-14 locks poll |
| EmailSender activity mail | In-app only | D-06; prefs not in scope |
| `notif.*` RPC prefix | `notification.*` | Prefer full word for readability — **recommend `notification.*`** |

**Installation:** None — no new crates.io / npm packages for Phase 17.

## Package Legitimacy Audit

No new package-manager installs planned. Table N/A.

## Architecture Patterns

### System flow (primary use case)

```
Actor mutates issue/PR (RPC)
  → require_verified + ACL Write
  → persist domain change
  → notify::fanout(recipients − actor, reason, subject)
       → INSERT notifications rows
Client (signed-in)
  → Query: notification.unreadCount → badge
  → /notifications: notification.list → rows
  → click: notification.markRead + navigate subject URL
```

### Recommended layout

```
crates/oxidean-db/migrations/{postgres,mysql,sqlite}/00NN_notifications.sql
crates/oxidean-db/src/notifications.rs
crates/oxidean-core/src/notification_types.rs
crates/oxidean-api/src/notification/mod.rs   # list/mark/unreadCount
crates/oxidean-api/src/notify/mod.rs        # fanout + mention parse (shared)
apps/web/src/lib/notification-queries.ts
apps/web/src/routes/notifications.tsrx
apps/web/src/components/chrome.tsrx          # bell
```

### Pattern: Fan-out after successful write
**What:** After `insert_issue` / comment / assignees / close succeed, call `notify::fanout` with computed recipient set.
**When:** Every D-01/D-02 event; failures in fan-out should log + soft-fail (do not roll back the user-visible mutation) unless already in the same DB transaction — prefer best-effort after commit of domain write so issue create never fails solely because notification insert failed. `[ASSUMED]` soft-fail preference — planner should lock best-effort after domain success.

### Pattern: Own-rows-only reads
**What:** All list/mark queries filter `recipient_id = session.user_id`.
**When:** Every notification RPC (D-15). Never accept `recipient_id` from client input.

### Anti-Patterns
- **Email fan-out via EmailSender** — violates D-06
- **Notifying the actor** — violates D-03
- **Dialect SQL in api crate** — project rule
- **Hand-edit api-client** — must `make rpc-gen`
- **Websockets** — violates D-14
- **Relying on markdown HTML for mentions** — client currently disables mention autolinks `[VERIFIED: apps/web/src/lib/markdown.issues.test.ts:35-41]`; parse plain text `@username` on the server

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Session auth | Custom JWT | Existing cookie `RpcCtx.session` + `require_verified` where writes already use it | Consistent with issues |
| Unread badge cache | Zustand store | TanStack Query + `session-queries` pattern | Project convention |
| Username resolve | New search index | `find_user_by_username` | Already shipped |
| Pagination | Infinite scroll | Offset + limit like `IssueListRequest` | D-09 + Issues parity `[VERIFIED: crates/oxidean-core/src/issue_types.rs:338-340]` |

## Schema recommendation (discretion)

```sql
-- logical: 00NN_notifications
CREATE TABLE IF NOT EXISTS notifications (
  id               TEXT PRIMARY KEY,
  recipient_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  actor_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  reason           TEXT NOT NULL,
  subject_kind     TEXT NOT NULL, -- 'issue' | 'pull_request'
  subject_repo_id  TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  subject_number   BIGINT NOT NULL,
  subject_title    TEXT NOT NULL DEFAULT '',
  read_at          TIMESTAMPTZ NULL,
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT notifications_subject_kind_check CHECK (subject_kind IN ('issue', 'pull_request'))
);
CREATE INDEX IF NOT EXISTS idx_notifications_recipient_created
  ON notifications (recipient_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_recipient_unread
  ON notifications (recipient_id, created_at DESC)
  WHERE read_at IS NULL;
```

**Reason strings (recommended enum):** `issue_opened`, `issue_closed`, `issue_reopened`, `issue_comment`, `issue_assigned`, `issue_unassigned`, `issue_mention`, `pr_opened`, `pr_closed`, `pr_reopened`, `pr_merged`, `pr_review`, `pr_comment`, `pr_review_requested`, `pr_mention`.

## Integration points (verified)

| Hook site | File / symbol | Events |
|-----------|---------------|--------|
| Issue create | `issue::create` `[VERIFIED: crates/oxidean-api/src/issue/mod.rs:157]` | opened + body mentions |
| Issue close/reopen | `issue::close` / `issue::reopen` `[VERIFIED: crates/oxidean-api/src/issue/mod.rs:317,336]` | closed / reopened |
| Comments | `issue.comments.create` in same module | comment + mentions; notify author, assignees, prior commenters |
| Assignees | `issue.assignees.set` | assigned / unassigned delta |
| PR module | **Absent on this branch** | Plan 17-03 precondition: Phase 12 PR handlers exist |

## Common Pitfalls

| Pitfall | Mitigation |
|---------|------------|
| Actor self-notify | Always `recipients.remove(actor_id)` |
| Fan-out fails mutation | Best-effort after domain success; log errors |
| Cross-user list leak | Force `recipient_id = session` in SQL; never trust client id |
| Migration number collision | At execute time pick next free after whatever Phase 12/etc. added |
| PR hooks before Phase 12 | Precondition halt; do not invent PR tables |
| Mention false positives in code fences | Prefer simple `@[A-Za-z0-9-]{1,39}` outside fenced blocks if cheap; else accept simple regex + document |

## Security notes (ASVS L1)

| Threat | Severity | Disposition |
|--------|----------|-------------|
| IDOR list/mark other users' notifications | high | mitigate — bind recipient to session |
| Unauthenticated unreadCount enumeration | medium | mitigate — require session; return 401 |
| Notification body XSS in UI | medium | mitigate — render as text / existing sanitize if markdown used |
| Private repo subject leakage to non-members | high | mitigate — only emit to users who already participate; subject title already known to them; if user loses access later, list still shows historical row but navigation uses existing `repo.not_found` anti-enum |

## Validation strategy hints

- API nextest: `notification_rpc.rs` — list empty, create via issue comment → unreadCount, markRead, markAllRead, cannot mark another's id, actor not notified
- Dialect: `dialect_notifications.rs` — migration applies on sqlite/postgres/mysql
- Web Vitest: chrome bell when signed in; `/notifications` Unread|All; mark read navigation
- Prior verify style: `cargo nextest run -p oxidean-api -E 'test(notification)'`; `bun`/`vitest` filters matching `notification`

## Open questions for planner (discretion only)

1. Soft-fail fan-out vs transactional insert — **recommend soft-fail after domain success**
2. Bell dropdown — **recommend link-only + badge** first (page mandatory); skip dropdown unless leftover context
3. Migration number — **resolve at execute** from next free `00NN`
