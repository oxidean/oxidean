# NOT-01 outbound notification email — spec

Work item: WI-0005 · Roadmap: NOT-01 · Direction: Option A
(`artifacts/options/A-chokepoint-detached-send.md`,
`context/decisions/notify-email-dispatch.md`)

## Context

Notification fan-out (`oxidean-api-core::notify::fanout_inner`) inserts one
inbox row per recipient after the repo watch matrix
(`all`/`participating`/`ignore`) has already filtered the set. Outbound mail
exists (`EmailSender` + log/SMTP/Resend adapters, hot-swap slot, admin
provider settings, Mailpit in dev compose) but serves auth mail only.
NOT-01 delivers each inbox-bound notification as an email to the
recipient's primary verified address — GitHub-parity "notifications get
emailed" — with zero new user-pref surface (that is the NOT-04 tail).

## Decision

1. **Dispatch point** — `fanout_inner` (the single funnel for
   `fanout`, `fanout_activity{,_with_watch,_including_actor}`,
   `fanout_suppress_ignored{,_with_watch}`) spawns email delivery for the
   same deduped recipient set it inserts rows for. Watch prefs are
   inherited unchanged.
2. **Detachment** — after successful inserts, one `tokio::spawn` per
   fan-out event performs: batch address resolution → per-recipient
   `OutboundEmail` build → sequential `send` with per-send warn-and-
   continue. RPC latency and transaction posture are unchanged; mail
   failure never fails the request.
3. **Address policy** — recipient address is the user's **primary**
   `user_emails` row with `verified_at IS NOT NULL`. No verified primary →
   silently skip that recipient (inbox row still exists).
4. **Mailer plumbing** — new `notify::mail` module, zero fan-out signature
   changes. `fanout_inner` ends with
   `mail::maybe_spawn(db, actor_id, &targets, reason, subject)`, mirroring
   the webhook `deliver::spawn_deliver` precedent (D-HOOK-11 fire-and-
   forget): env flag checked cheaply, then `tokio::spawn` a task holding a
   cloned `Database`. Inside the task, `resolve_sender(&db)` loads
   `db.get_auth_settings()` → `build_email_sender_for_settings` (fallback
   `build_email_sender_from_env` on lookup failure) — fresher than any
   in-request snapshot and identical in RPC, sweep, and scheduler contexts
   (all have `db`). The testable seam is `dispatch(db, sender, origin,
   actor, recipients, reason, subject)` — unit tests drive it with a
   recording `EmailSender`.
5. **Kill-switch** — `OXIDEAN_NOTIFY_EMAIL_ENABLED` (default on, parsed
   with the `mcp_env_truthy` convention). Disabled → `maybe_spawn` returns
   before spawning — no DB work, no task. Documented in
   `docs/CONFIGURATION.md` mail section + `.env.example` comment.
6. **Message shape** — pure builder
   `build_notification_email(...) -> OutboundEmail` in `notify::mail`:
   - Subject: `[{owner}/{repo}] {title} ({tail})` where tail is `#n` for
     numbered subjects (issue/pull_request), the ref/tag otherwise, omitted
     when empty.
   - Text body (required): one line "{actor} {reason-phrase}:" + subject
     line + absolute URL + a trailing "You are receiving this because you
     watch {owner}/{repo} — manage at {origin}/settings/notifications".
   - HTML (optional): same content, single `<p>` per line, linked URL.
     `OutboundEmail.html` stays `Some(_)` only when built — text is the
     contract, HTML is best-effort polish.
   - Reason phrases mirror `reasonLabel()` in
     `apps/web/src/lib/notification-queries.ts` (issue_*, pr_*,
     release_*, workflow_run_*) — a Rust `reason_phrase()` in
     `notify::mail`, same wording.
   - Absolute links mirror `subjectHref()` path shapes using
     `resolve_public_origin()` once per event, with owner slug + repo name
     resolved via one `find_repository_by_id` + owner-type join per event
     (the same join `oxidean-api-notification::owner_slug_for_repo` does —
     extracted into `notify::mail` so both can share it later).
   - Actor display name via one `find_user_by_id(actor_id)` per event,
     falling back to a neutral phrase ("someone") on lookup failure.
   - Deleted/unresolvable repo at dispatch time → log + skip mail (row
     stays).
7. **New DB query** — `user_emails::list_primary_verified_emails_for_users
   (pool, &[String]) -> Result<Vec<(user_id, email)>, String>` in all three
   dialects (pattern: `list_verified_emails_for_users` + `is_primary`), plus
   `Database::list_primary_verified_emails_for_users` façade.
8. **No RPC / wire changes** — nothing new is exposed; `make rpc-gen` is
   expected to be a no-op (run `make rpc-sync-check` to prove it).

## Out of scope (later items)

- Digests / batching (NOT-02) and reply-by-email threading (NOT-03) —
  design admits them via the `notify::mail` dispatch seam and future optional
  headers on `OutboundEmail`; not built.
- Per-user email prefs, global mute, per-type filters (NOT-04 tail).
- Durable queue/retry, delivery receipts, bounce handling.
- Admin UI changes (provider surface already exists).

## Acceptance criteria

1. A comment on an issue in a repo watched at `all` produces the inbox row
   AND one email to the commenter's-excluded recipient primary-verified
   address, subject containing `owner/repo` and `#n`, body containing an
   absolute link under `OXIDEAN_PUBLIC_ORIGIN`.
2. `ignore` level suppresses mail exactly as it suppresses rows;
   `participating` unchanged. Actor exclusion unchanged (workflow-run
   fan-out still includes the actor).
3. Recipient without verified primary address → no mail, no error.
4. `OXIDEAN_NOTIFY_EMAIL_ENABLED=0` → rows still inserted, zero sends.
5. Sender error → warn log; inserts and the request are unaffected.
6. New batch query has dialect coverage (sqlite unit test minimum, plus
   `dialect_*` parity test where the harness supports it).
7. Mail-builder unit tests cover each `subject_kind` path shape and at
   least one reason phrase per family; a recording `EmailSender` test
   double proves per-recipient sends and skip-on-unverified.
8. `cargo nextest run -p oxidean-api-core -p oxidean-db` green;
   `make rpc-sync-check` clean; CONFIGURATION.md + `.env.example` document
   `OXIDEAN_NOTIFY_EMAIL_ENABLED`; `docs/dev-auth.md` gains a line that
   `make up-with-dev-auth` + Mailpit shows notification mail.
9. No changes under `packages/api-client` (no wire diff).

## Verification hooks

- Recording `EmailSender` double in `notify::mail` tests captures
  `OutboundEmail` vec for assertions.
- Dispatch honors `OXIDEAN_NOTIFY_EMAIL_ENABLED=0` in `maybe_spawn`, so
  unit tests never touch the network; `dispatch` takes an explicit
  `Arc<dyn EmailSender>` so tests inject a recording double.
- CI e2e can assert via Mailpit's HTTP API the same way auth mail tests do.
