# Option A — chokepoint send, detached dispatch, watch-prefs only

## Shape

- `notify::fanout_inner` is the single send point. After the per-recipient
  `insert_notification` batch, it also kicks off email delivery for the same
  recipient set — the watch matrix has already run, so email inherits
  `all`/`participating`/`ignore` semantics exactly like the inbox row.
- Thread the sender through the `fanout*` family as a new parameter
  (`mailer: &Arc<dyn EmailSender>` or a small `NotifyMailer` struct wrapping
  sender + db + public origin) — ~20 call sites update mechanically, each
  already holds `ctx`/`app` and can pass `ctx.email` or `app.current_email()`.
- Delivery is a detached `tokio::spawn` per fan-out event (not per
  recipient): one task resolves addresses once (`oxidean-db` batch
  primary+verified query), builds one `OutboundEmail` per recipient, and
  sends sequentially (or `join_all` with a small concurrency cap). Per-send
  failure logs and continues — same soft-fail posture as row inserts.
- Templates: new `notify::mail` (or `email::templates::notification`)
  module builds subject/text (+ optional HTML) from
  `NotifySubject` + `reason` + actor username + absolute link
  (`public_origin::resolve_public_origin()` + owner-slug join, mirroring
  `subjectHref`).
- No new RPC, no new pref surface, no web UI in this slice. User-level mail
  prefs (global mute / per-type) stay the NOT-04 tail.

## Tradeoffs

+ Smallest diff that delivers the roadmap item; ~all call sites untouched
  semantically (one added arg).
+ Prefs are exactly consistent with inbox — no second matrix to drift.
+ Detached spawn keeps RPC latency unchanged; failure blast radius = log
  lines.
+ No DB migration; batch query is a pure addition to `user_emails.rs`.
− Detached sends are fire-and-forget: crash between insert and send loses
  the mail (inbox row remains — acceptable per GitHub-parity posture;
  transient SMTP failure also loses it).
− No user opt-out for email specifically until NOT-04 — a user at watch
  level `all` gets mail for every event (GitHub default is the same; env
  kill-switch mitigates).
− `tokio::spawn` in api-core means tests need a recording sender (trait
  seam makes this easy).

## Forward-compat with NOT-02/NOT-03

- `NotifyMailer` boundary is where a digest queue can later sit (enqueue
  instead of send).
- Reply-by-email needs `Message-ID`/`Reply-To` with per-notification
  threading tokens — `OutboundEmail` can grow optional `headers`/`reply_to`
  fields without breaking adapters.

## Kill-switch

- Honor an env master switch (`OXIDEAN_NOTIFY_EMAIL_ENABLED`, default on)
  checked once at dispatch — gives self-hosters an off and gives tests a
  clean disable. Documented in CONFIGURATION.md alongside the other
  `OXIDEAN_*` mail vars.
