# WI-0005 discovery — NOT-01 outbound notification email

Scope per roadmap `NOT-01` (`.ox-agent/context/product/roadmap.md:110`):
"Outbound email for watched/assigned/mentioned issue and PR activity. Email
infra exists (`EmailSender`: SMTP/Resend/log) but is wired to auth mail only;
notification fan-out writes DB rows, no email send." Digests (NOT-02) and
reply-by-email (NOT-03) are later items — design must not preclude them, but
do not build them.

## What already exists (reuse, don't rebuild)

### Outbound mail adapters — `crates/oxidean-api-core/src/email/`

- `EmailSender` trait (`async fn send(&self, msg: OutboundEmail)`),
  `OutboundEmail { to, subject, text, html }` — `email/mod.rs:17-39`.
- Adapters: `LogSink` (log_sink.rs), `SmtpSender` (lettre, smtp.rs),
  `ResendSender` (resend.rs, optional `OXIDEAN_RESEND_BASE_URL` override for
  local stubs).
- Selection: `build_email_sender_for_settings(&AuthSettingsRow)`
  (mod.rs:55) — DB `auth_settings.email_provider` ∈ `log|smtp|resend` +
  `from_address`; secrets stay in ENV (`OXIDEAN_SMTP_URL`,
  `OXIDEAN_RESEND_API_KEY`, `OXIDEAN_MAIL_FROM`).
  `build_email_sender_from_env()` (mod.rs:126) is the boot-time fallback.
- Hot-swap: `AppState.email: Arc<RwLock<Arc<dyn EmailSender>>>`
  (`app.rs:27`), `AppState::current_email()` (app.rs:337), mirrored onto
  `RpcCtx.email` + `RpcCtx.email_slot` (`rpc.rs:120-122`). Admin auth-settings
  update rebuilds the sender into the slot.
- Existing send precedent: `auth/verify_reset.rs:307` — `ctx.email.send(msg)`
  awaited in-request, error logged (soft-fail).

### Admin surface — already built

`oxidean-api-admin/src/settings.rs`: `admin.getAuthSettings` /
`admin.updateAuthSettings` own `email_provider` (validated
`log|smtp|resend`, line 40-47), `from_address`, and report
`smtp_configured`/`resend_configured` env flags (line 107+). NOT-01 needs no
new admin surface — possibly doc touch-ups only.

### Notification fan-out — `crates/oxidean-api-core/src/notify/mod.rs`

- Every fan-out funnels through `fanout_inner` (mod.rs:30) — one insert per
  recipient via `db.insert_notification`, soft-fail per recipient.
- Watch matrix applied upstream of `fanout_inner`:
  `fanout_activity{,_with_watch,_including_actor}` and
  `fanout_suppress_ignored{,_with_watch}` resolve `all`/`participating`/
  `ignore` rows from `db.list_repo_watch_levels` before insertion
  (mod.rs:95-138). So the recipient set that lands in `fanout_inner` is
  already the user-pref-respecting set — email send at that chokepoint
  inherits watch prefs for free.
- ~20 call sites across `oxidean-api-issues`, `-pulls`, `-release`,
  `-actions`, `-org`, `-user`, `-repo` (all pass `&Database` + actor +
  recipients + reason + `NotifySubject`).
- Reasons enumerated in `apps/web/src/lib/notification-queries.ts`
  `reasonLabel` (issue_*/pr_*/release_*/workflow_run_*).

### Recipient → address resolution — `crates/oxidean-db/src/user_emails.rs`

- `find_primary_for_user(pool, user_id) -> Option<UserEmailRow>` (line 600) —
  per-user primary lookup (has `verified_at`).
- `list_verified_emails_for_users(pool, &[user_ids])` (line 274) — batch,
  verified only, but returns **all** verified addresses, not primary-only.
- Gap: no batch "primary verified email for many users" — either add a
  dialect query in `oxidean-db` (all dialect SQL lives there) or accept
  N+1 `find_primary_for_user`. Batch is the right shape for `all`-watcher
  fan-outs.

### Deep links

- `public_origin::resolve_public_origin()` (api-core) — used by OAuth /
  actions sweeps for absolute URLs in mail.
- Repo owner-slug join exists in
  `oxidean-api-notification/src/handlers.rs` `owner_slug_for_repo` (line 32):
  `owner_type` → org name or user username + `repo.name`.
- Path shape to mirror: `subjectHref` in
  `apps/web/src/lib/notification-queries.ts:52` —
  `/{owner}/{repo}/{issues|pull}/{n}`, `/releases/{tag}`, `/actions/{run}`,
  `/commits/{ref}`.

### Dev stack

- Mailpit already in dev-auth Compose: `smtp://mailpit:1025`, UI `:8025`
  (`docs/dev-auth.md:55`). `make up-with-dev-auth` promotes DB
  `email_provider` log→smtp (dev-auth.md:88). e2e auth tests already assert
  against Mailpit's HTTP API — reuse that pattern for notification-mail
  e2e.
- Env contract documented in `docs/CONFIGURATION.md` (lines 33-36, 122, 346)
  and `.env.example:103-106`, `docs/dev-auth.env.example`.

## What does not exist (the NOT-01 gap)

- No mail send anywhere in the notify path — `fanout_inner` inserts rows and
  stops.
- No user-level email-notification pref of any kind (global mute, per-type
  filters, email prefs are explicitly the NOT-04 tail). Watch levels are the
  only pref model today.
- No notification-mail template module (subject/body builders like
  `build_verify_email`/`build_reset_email` in verify_reset.rs:136-165).
- No async dispatch mechanism — auth mail is awaited inline; fan-out to N
  recipients inline would add request latency (decision for spec/plan:
  `tokio::spawn` detached sends keeps soft-fail semantics).

## Constraints & standards that bind

- Trait+adapter seam precedent (`standing-decisions.md:45`,
  `standards/rust.md:16`): mail building/sending lives in `oxidean-api-core`
  modules, not in `oxidean-api` shell or per-domain crates.
- Dialect SQL only inside `oxidean-db` (AGENTS.md hard boundary) — any new
  batch email query goes to `crates/oxidean-db/src/user_emails.rs`.
- No `services/` layer — domain internals are items on `AppState`/`RpcCtx`
  (`standards/rust.md`).
- Commits `type(scope): subject`; relevant scopes: `api-core`, `db`,
  `api-notification`, `web`, `docs` (`versioning.toml`).
- Test minimums (`standards/testing.md`): helper → unit test; RPC contract →
  Rust integration test; dialect-touching DB → `dialect_*` test.
- Scratch in `tmp/`; no repo-wide format; `git add` explicit paths.
- `make rpc-gen` only if Rust RPC types change — notification email may not
  need new RPC procedures at all (no new prefs UI in this slice → possibly
  zero wire changes; spec stage decides whether a minimal "email me"
  opt-out lands here or defers to NOT-04).

## Open questions for spec stage

1. Send timing: detached `tokio::spawn` per fan-out vs a queued drain —
   recommendation: spawn detached, preserves in-request soft-fail semantics
   and matches GitHub async mail.
2. Address policy: primary+verified only (GitHub parity) — needs a batch
   primary-verified query in `oxidean-db`.
3. Email-level opt-out for NOT-01: ship a minimal per-user "email
   notifications" toggle now, or ride watch prefs only and defer all mail
   prefs to NOT-04? (Roadmap wording suggests the latter; a kill-switch env
   or settings toggle is cheap insurance either way.)
4. HTML vs text-only first pass; template module placement
   (`email/templates` vs `notify/mail.rs`).
