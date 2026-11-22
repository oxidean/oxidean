# ADR: notification email dispatch (WI-0005 / NOT-01)

Status: confirmed by WI-0005 spec (`artifacts/spec.md`) · Date: 2026-11-22

## Context

The notification fan-out (`oxidean-api-core::notify`) inserts one inbox row
per recipient after applying the repo watch matrix
(`all`/`participating`/`ignore`). Outbound mail infrastructure
(`EmailSender`, SMTP/Resend/log adapters, hot-swap slot on
`AppState`/`RpcCtx`, admin provider settings) already exists but is wired
to auth mail only. NOT-01 turns fan-out events into email; NOT-02 (digests)
and NOT-03 (reply-by-email) are later items that will want scheduling and
threading.

## Decision

Send notification email at the fan-out chokepoint, detached:

- `notify::fanout_inner` dispatches email for the same recipient set it
  inserts rows for — one place, watch prefs already applied, all call-site
  variants (`fanout`, `fanout_activity*`, `fanout_suppress_ignored*`)
  covered.
- Delivery runs in a detached `tokio::spawn` per fan-out event (webhook
  `deliver::spawn_deliver` precedent, D-HOOK-11): batch address
  resolution, per-recipient `OutboundEmail`, sequential send with
  per-send soft-fail logging. In-request latency is unchanged. The task
  resolves the sender itself from `db.get_auth_settings()` →
  `build_email_sender_for_settings` (env fallback), so dispatch honors
  current admin settings from RPC, sweep, and scheduler contexts alike —
  no signature change at the ~32 fan-out call sites.
- Recipient address = the user's **primary verified** address, resolved by
  a new batch query in `oxidean-db` (`user_emails`), not per-user lookups.
- No user-level mail pref ships in NOT-01. Watch levels remain the only
  pref model; an env master switch (`OXIDEAN_NOTIFY_EMAIL_ENABLED`,
  default on) is the operator kill-switch. Per-user mute/per-type mail
  prefs remain the NOT-04 tail.
- The `notify::mail` dispatch boundary holds the send point so a durable
  queue (Option C) can replace detached sends later without touching call
  sites.

## Alternatives considered

- **Per-user email toggle in this slice** — rejected: pulls NOT-04 pref
  work forward (migration + RPC + UI), and a single bool would be reshaped
  by NOT-04's matrix anyway. Env switch + watch prefs match GitHub
  defaults.
- **Durable `outbound_mail` queue + drain worker** — deferred, not
  rejected: it is the right substrate once NOT-02 digests need scheduling
  and retry guarantees matter, but it is architecture ahead of its
  consumer for a fire-and-forget parity feature.

## Consequences

- Crash or transient SMTP failure can lose an email (the inbox row
  survives). Accepted for parity posture; revisit when the queue lands.
- Fans at watch level `all` on busy repos get one mail per event — no
  batching/digest until NOT-02.
- `OutboundEmail` may grow optional headers (`Message-ID`, `Reply-To`,
  `List-Unsubscribe`) when NOT-03 needs threading; adapters absorb it.

## References

- Discovery + options:
  `.ox-agent/work/active/WI-0005-not-01-outbound-notification-email/artifacts/`
- Roadmap `NOT-01` (`.ox-agent/context/product/roadmap.md`)
