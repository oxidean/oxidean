# Phase 17: Notifications - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-16
**Phase:** 17-Notifications
**Mode:** `--auto` (campaign defaults)
**Areas discussed:** Triggers & recipients, Delivery channel, UI chrome & list, Read/unread model, Transport & auth

---

## Triggers & recipients

| Option | Description | Selected |
|--------|-------------|----------|
| GitHub participant rules | Author, assignees, commenters, @mentions, requested reviewers; skip actor; issue+PR core events | ✓ |
| Watch-all collaborators | Notify every Write+ on every event | |
| Author-only | Only notify issue/PR author | |

**User's choice:** [auto] GitHub participant rules (recommended default)
**Notes:** PR event set taken from Phase 12 CONTEXT. Reactions/labels deferred as noisy (D-04).

---

## Delivery channel

| Option | Description | Selected |
|--------|-------------|----------|
| In-app only | List + badge; no EmailSender activity mail | ✓ |
| In-app + email always | Fan-out via EmailSender on every event | |
| In-app + optional email | Prefs UI + templates | |

**User's choice:** [auto] In-app only — EmailSender exists but prefs/templates are not "already easy"
**Notes:** Campaign default: "No email blast required unless already easy via existing EmailSender."

---

## UI chrome & list

| Option | Description | Selected |
|--------|-------------|----------|
| Header bell + `/notifications` page | Unread\|All, offset pagination, click → mark read + navigate | ✓ |
| Page only (no header badge) | Discoverability worse | |
| Dropdown-only (no dedicated page) | Weaker list/mark-all UX | |

**User's choice:** [auto] Header bell + dedicated page (recommended)
**Notes:** Optional dropdown preview left to Claude's discretion (D-11).

---

## Read/unread model

| Option | Description | Selected |
|--------|-------------|----------|
| read_at + mark one/all + unreadCount | Satisfies NOTF-02 | ✓ |
| Also Done/archive inbox | Extra GitHub surface | |
| Soft-delete only | No unread concept | |

**User's choice:** [auto] read_at + mark one/all + unreadCount

---

## Transport & auth

| Option | Description | Selected |
|--------|-------------|----------|
| Query poll for unreadCount; cookie session; own rows only | Matches chrome patterns | ✓ |
| Websockets / SSE push | Extra infra | |

**User's choice:** [auto] Poll + session auth

---

## Claude's Discretion

- RPC naming, schema/migration number, payload shape, bell dropdown vs link-only, poll interval, @mention parser sharing, optimistic mark-read

## Deferred Ideas

- Email digests / preference center
- Watch/unwatch
- Websockets
- Non-issue/PR notification types
- Formal shared event bus with Phase 18 webhooks
