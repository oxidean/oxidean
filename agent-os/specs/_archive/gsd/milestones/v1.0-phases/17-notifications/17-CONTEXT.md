# Phase 17: Notifications - Context

**Gathered:** 2026-09-16
**Status:** Ready for planning
**Discuss mode:** `--auto` (campaign defaults: GitHub-like in-app; auto-decide; Phase 12 CONTEXT for PR events)

<domain>
## Phase Boundary

Signed-in users stay aware of **issue and PR activity** via **in-app** notifications: create on relevant events, list, and mark read. Delivers NOTF-01, NOTF-02.

**Requirements:** NOTF-01, NOTF-02

**Success criteria (from ROADMAP):**
1. Signed-in user receives in-app notifications for relevant issue and PR activity
2. User can list notifications and mark them as read

**Out of scope (later phases / deferred):**
- Outbound email / digest / notification preference center (Phase 17 is in-app only — EmailSender exists but prefs + templates are not "already easy")
- Repo watch / unwatch / custom subscription matrix beyond GitHub-default participant rules
- Realtime websockets / SSE push (poll is enough)
- Actions / packages / release / LFS activity notifications
- Outbound webhooks (Phase 18) — share event taxonomy conceptually but do not implement delivery here
- Notification "Done" archive / unsubscribe threads (beyond mark read)

**UI hint:** yes — signed-in `SiteHeader` bell + `/notifications` list.

**Depends on:** Phase 11 (Issues — shipped), Phase 12 (Pull Requests — plan/execute wave; PR event hooks land when Phase 12 domain exists). Plan now; execute after Phase 12 lands / rebase as needed.

</domain>

<decisions>
## Implementation Decisions

### A — Triggers & recipients (GitHub-like)
- **D-01:** Emit in-app notifications for **issue** activity: opened, closed, reopened, commented, assigned, unassigned — **Reversibility:** costly — event hooks in issue write paths
- **D-02:** Emit for **PR** activity per Phase 12 CONTEXT: opened, closed, reopened, merged, review submitted (Approve / Request changes / Comment), general Conversation comments, line comments, **requested as reviewer** — **Reversibility:** costly — depends on Phase 12 PR domain call sites
- **D-03:** Recipients follow GitHub participant rules: **author**, **assignees**, **prior commenters/participants**, **@mentioned** users (markdown `@username`), **requested reviewers**; **never notify the actor** — **Reversibility:** reversible
- **D-04:** Skip noisy/low-signal events in v1: emoji reactions, label-only changes, force-push / retarget / draft toggles, merge-strategy settings — **Reversibility:** reversible
- **D-05:** Subject links use stable forge URLs: `/{owner}/{repo}/issues/{n}` and `/{owner}/{repo}/pull/{n}` (align Phase 12 URL discretion) — **Reversibility:** reversible

### B — Channel
- **D-06:** **In-app only** for Phase 17 — do **not** send EmailSender mail for activity notifications (adapters are for auth/org invites; email notifs need prefs/templates → defer) — **Reversibility:** reversible (deferral)
- **D-07:** No per-user notification preference UI in this phase — defaults above are fixed — **Reversibility:** reversible

### C — UI chrome & list
- **D-08:** Signed-in **`SiteHeader`** shows a **bell** control with **unread count badge**; links to `/notifications` — **Reversibility:** reversible
- **D-09:** `/notifications` page: filters **Unread | All**; sort **newest first**; **offset pagination** (match Issues/PRs list patterns) — **Reversibility:** reversible
- **D-10:** Each row shows reason text + repo + subject title/`#N`; activating a row **marks that notification read** and **navigates** to the subject — **Reversibility:** reversible
- **D-11:** Optional compact dropdown preview from the bell is **Claude's discretion** (page is mandatory; dropdown if cheap without duplicating list logic badly) — **Reversibility:** reversible

### D — Read / unread model
- **D-12:** Persist `read_at` (null = unread); RPC: **list** (filter + pagination), **markRead** (one or many ids), **markAllRead**, **unreadCount** for the badge — **Reversibility:** costly — schema + RPC surface
- **D-13:** NOTF-02 satisfied by list + mark read (+ mark all); no delete/archive/Done inbox in v1 — **Reversibility:** reversible

### E — Transport & auth
- **D-14:** Unread badge uses **TanStack Query** poll/refetch (chrome session pattern); **no websockets** in Phase 17 — **Reversibility:** reversible
- **D-15:** Notification RPCs require signed-in cookie session; users only see/mutate **their own** rows — **Reversibility:** reversible

### Claude's Discretion
- Exact RPC namespace (`notification.*` vs `notif.*`) and migration number after packages/releases
- Notification row payload shape (reason enum vs free-text reason_key + params)
- Whether bell opens a small dropdown preview or only navigates to `/notifications`
- Poll/staleTime interval for unreadCount
- How @mention parsing shares issue/PR markdown pipeline
- Whether mark-read-on-navigate is optimistic UI

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 17 goal, success criteria, depends on 11 + 12
- `.planning/REQUIREMENTS.md` — NOTF-01, NOTF-02
- `.planning/parallel-tracks/phase-17.md` — plan now; execute after Phase 12
- `.planning/phases/11-issues/11-CONTEXT.md` — issue lifecycle events deferred to Phase 17
- `.planning/phases/12-pull-requests/12-CONTEXT.md` — PR events / review model for notification triggers (D-PR-*)
- `.planning/phases/04-auth-sessions-email/04-CONTEXT.md` — session cookie + EmailSender (auth/org only; not activity email)
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Capability ACL for subject visibility

### Product / architecture
- `docs/ARCHITECTURE.md` — RpcCtx, SessionService, EmailSender placement
- `docs/API.md` — RPC conventions
- `.agents/skills/octane/SKILL.md` — Octane `.tsrx` UI (mandatory for UI plans)

### Code anchors
- `apps/web/src/components/chrome.tsrx` — `SiteHeader` / `AccountActions` (bell insertion point)
- `apps/web/src/lib/session-queries.ts` — Query session helpers for unreadCount pattern
- `crates/oxidean-api/src/email/mod.rs` — EmailSender trait (do **not** wire activity mail)
- `crates/oxidean-api/src/issue/` — issue write paths to hook emitters
- `crates/oxidean-db/migrations/` — next migration after `0015_packages.sql` (or later if Phase 12 adds PR migrations first)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `SiteHeader` / `AccountActions` in `chrome.tsrx` — signed-in chrome for bell + badge
- `authSessionQueryOptions` in `session-queries.ts` — pattern for soft Query RPCs in chrome
- Issue domain under `crates/oxidean-api/src/issue/` — hook points for emitters
- `EmailSender` — available but **not** used for activity notifications this phase
- Offset pagination + Unread/All style filters already established on Issues lists

### Established Patterns
- Rust Axum JSON RPC → `make rpc-gen` → `@oxidean/api-client`
- Dialect SQL only in `oxidean-db`; API must not branch on dialect
- Octane `.tsrx` + TanStack Query for server state; no Zustand for session-like data
- Cookie session auth via `RpcCtx` / `SessionService`

### Integration Points
- Issue create/comment/assign/close/reopen handlers → notification fan-out
- Phase 12 PR open/review/comment/merge/close handlers → same fan-out (when present)
- `SiteHeader` signed-in cluster (ThemeSelect / AccountActions) → bell
- New route `/notifications` under TanStack Start file routes

</code_context>

<specifics>
## Specific Ideas

- Campaign lock: **GitHub-like in-app** notifications for issue + PR activity; list + mark read.
- **No email blast** unless already easy via EmailSender — decided **not** easy (needs prefs/templates) → in-app only.
- Use **Phase 12 CONTEXT** for which PR events count (reviews, line comments, requested reviewers, merge).
- Parallel track: plan on this branch; execute after Phase 12 lands.

</specifics>

<deferred>
## Deferred Ideas

- Email / digest notifications and preference center
- Repo watch / custom subscription matrix
- Websockets / SSE realtime push
- Actions, packages, releases, LFS notifications
- GitHub-style "Done" / unsubscribe from thread
- Sharing a formal event bus with Phase 18 webhooks (conceptual overlap only)

</deferred>

---

*Phase: 17-Notifications*
*Context gathered: 2026-09-16*
*Auto-log: [--auto] Selected all gray areas: Triggers & recipients, Delivery channel, UI chrome & list, Read/unread model, Transport & auth.*
