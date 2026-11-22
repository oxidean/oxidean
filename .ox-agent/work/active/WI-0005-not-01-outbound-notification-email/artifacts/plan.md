# NOT-01 outbound notification email — plan

Spec: `artifacts/spec.md` · ADR: `context/decisions/notify-email-dispatch.md`
One implementation PR on `feat/not-01-outbound-email`. Order below is the
suggested build sequence inside the branch.

## Task 1 — DB: batch primary-verified email query

- `crates/oxidean-db/src/user_emails.rs`: add
  `list_primary_verified_emails_for_users(pool, &[String]) ->
  Result<Vec<(String, String)>, String>` for all three dialects — same
  pattern as `list_verified_emails_for_users` (line 274) plus
  `is_primary` (`= true` / `= 1` / `= 1`) and `verified_at IS NOT NULL`.
  Empty input → `Ok(vec![])` early.
- `crates/oxidean-db/src/lib.rs`: add `Database::list_primary_verified_
  emails_for_users(&self, user_ids: &[String])` façade next to
  `list_verified_emails_for_users` (~line 3733).
- Tests: sqlite unit coverage in `user_emails.rs` tests (seed users with
  primary-verified / primary-unverified / secondary-verified rows → only
  primary-verified returned); check `crates/oxidean-db/tests/` for an
  existing dialect-parity harness covering `user_emails` and extend it.

Gate: `cargo nextest run -p oxidean-db` green (user_emails filters).

## Task 2 — `notify::mail` module + fan-out hook

- `crates/oxidean-api-core/src/notify/mail.rs` (new file):
  - `env_notify_email_enabled() -> bool` — `OXIDEAN_NOTIFY_EMAIL_ENABLED`,
    `mcp_env_truthy`-style parse, default true.
  - `resolve_sender(db: &Database) -> Arc<dyn EmailSender>` —
    `db.get_auth_settings()` → `email::build_email_sender_for_settings`;
    Err → `email::build_email_sender_from_env` + warn.
  - `subject_path(subject, owner_slug, repo_name) -> String` — mirrors
    `subjectHref` (issue/pull_request → `…/issues|pull/{n}`; release →
    `…/releases/{ref}`; workflow_run → `…/actions/{ref}`; push →
    `…/commits/{ref}`).
  - `subject_tail(subject)` — `#n` for numbered kinds, `subject_ref`
    otherwise, `""` for workflow_run (matches `subjectTail`).
  - `reason_phrase(reason) -> &str` — mirrors `reasonLabel`.
  - `resolve_subject_context(db, repo_id, actor_id)` — one
    `find_repository_by_id` + owner-type join (org name / user username,
    same shape as `oxidean-api-notification::owner_slug_for_repo`) + one
    `find_user_by_id`; `None` on unresolvable repo → caller skips mail.
  - `build_notification_email(to, ctx) -> OutboundEmail` — pure builder:
    subject `[{owner}/{repo}] {title} ({tail})` (tail omitted when
    empty); text body per spec; minimal HTML (`html: Some(_)`).
  - `dispatch(db, sender, origin, actor_name, recipients, reason,
    subject, link_ctx)` — batch
    `list_primary_verified_emails_for_users` → per-recipient build +
    sequential `send`, warn-and-continue per failure.
  - `maybe_spawn(db, actor_id, recipients, reason, subject)` — env check
    → `tokio::spawn` the resolve→dispatch pipeline (mirrors
    `webhook::deliver::spawn_deliver`).
- `notify/mod.rs`: `mod mail;` + call `mail::maybe_spawn(&db, actor_id,
  &targets, reason, subject)` at the end of `fanout_inner` (after
  inserts — only when `!targets.is_empty()`).
- Unit tests in `mail.rs`: `build_notification_email` per subject_kind +
  reason families; `subject_tail`/`subject_path` table tests;
  `env_notify_email_enabled` truth table; `dispatch` with a recording
  `EmailSender` double + sqlite test DB asserting per-recipient sends,
  skip-on-unverified, send-failure continuation.
- Check how existing api-core/db tests build a sqlite `Database` (search
  `Database::connect("sqlite` / test helpers) and reuse it.

Gate: `cargo nextest run -p oxidean-api-core` green; `cargo clippy -p
oxidean-api-core --all-targets` clean.

## Task 3 — Docs + env example

- `docs/CONFIGURATION.md`: `OXIDEAN_NOTIFY_EMAIL_ENABLED` row in the mail
  table (~line 36) and the source-map table (~line 346).
- `.env.example`: commented `OXIDEAN_NOTIFY_EMAIL_ENABLED=` near the mail
  block (~line 106).
- `docs/dev-auth.md`: one line noting `make up-with-dev-auth` +
  Mailpit UI shows notification mail (e.g. on issue comments in watched
  repos).

## Task 4 — Gates + PR

- `make rpc-sync-check` (expect clean — no wire changes).
- `cargo fmt --check` scoped files; `clippy` on touched crates;
  `cargo machete` if dep graph changed (shouldn't — no new deps).
- `git merge origin/main` before PR; commits:
  `feat(db): batch primary-verified email lookup`,
  `feat(api-core): email notification fan-out recipients`,
  `docs: document OXIDEAN_NOTIFY_EMAIL_ENABLED`.
- `gh pr create` + `@coderabbitai review` comment.

## Risks / assumptions

- 32 call sites stay untouched — the riskiest alternative (param
  threading) is avoided by resolving the sender inside the task.
- `find_auth_settings` name is `get_auth_settings` (verified,
  `oxidean-db` lib.rs:6272).
- Auth-mail snapshot (`ctx.email`) vs task-time settings: notification
  mail intentionally reads settings at dispatch — freshest config, and
  matches the detached posture. Inbound `ctx.email` is unchanged.
- Fan-outs on repo deletion races: mail resolves context lazily → skip +
  log; row survives.
- Concurrency: sends are sequential inside one task per event — no
  rate-limit storm for Mailpit/SMTP on large `all`-watch fan-outs; a
  recipient cap is unnecessary at forge scale but noted for NOT-02.
