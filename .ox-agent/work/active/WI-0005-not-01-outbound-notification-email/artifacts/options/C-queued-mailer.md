# Option C — durable outbound-mail queue + worker

## Shape

- New `outbound_mail` table (`id, recipient_id, to_addr, subject, text,
  html, status, attempts, next_attempt_at, created_at, sent_at,
  last_error`) — migration + dialect accessors in `oxidean-db`.
- `fanout_inner` enqueues one row per recipient inside the same soft-fail
  envelope (address resolution still needed at enqueue or drain time).
- A background task on `AppState` (tokio interval worker) claims due rows,
  sends via the current `EmailSender`, marks sent / schedules retry with
  backoff (bounded attempts, dead-letter to `failed`).
- Env: `OXIDEAN_NOTIFY_EMAIL_ENABLED` plus worker tuning knobs
  (batch size, interval) if exposed.

## Tradeoffs

+ Survives crashes and transient SMTP outages — the only option with real
  delivery guarantees.
+ The queue is exactly what NOT-02 digests and NOT-03 reply-by-email will
  want (scheduled sends, dedupe windows).
− Largest diff: migration, worker lifecycle, retry semantics, observability
  surface (metrics/logging conventions), three-dialect test coverage.
− Over-builds for "email on watched activity" — the roadmap positions
  NOT-01 as the unlock, not the platform. A queue can land with NOT-02
  (digests) which is the first consumer that actually needs scheduling.
− In-request enqueue still leaves the row→mail gap if drain is slow —
  perceived latency similar to detached send for the happy path.

## When this wins

If spec decides mail loss on restart is unacceptable for a forge, or wants
the NOT-02/03 queue paid for now. Otherwise it's architecture ahead of its
consumer — record it as the NOT-02 design direction rather than build it.
