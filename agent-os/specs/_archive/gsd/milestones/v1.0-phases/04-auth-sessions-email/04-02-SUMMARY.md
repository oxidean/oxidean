---
phase: 04-auth-sessions-email
plan: "02"
subsystem: auth
tags: [email, lettre, reqwest, resend, smtp, log-sink, wiremock]

requires:
  - phase: 04-auth-sessions-email
    provides: "Auth schema/DTOs from 04-01 (welcome wiring lands in 04-04)"
provides:
  - "EmailSender trait + OutboundEmail + EmailError"
  - "LogSink (AUTH-09) via tracing target oxidean.mail"
  - "SmtpSender via lettre AsyncSmtpTransport (AUTH-10)"
  - "ResendSender via reqwest + User-Agent oxidean-api/0.1 (AUTH-11)"
  - "build_email_sender_from_env selection: Resend → SMTP → LogSink"
affects:
  - 04-04-local-auth-rpc
  - 04-06-profile-admin

tech-stack:
  added: [lettre 0.11.23, reqwest 0.13.5, async-trait, thiserror 2, wiremock]
  patterns:
    - "EmailSender trait with Arc<dyn EmailSender> env factory"
    - "lettre typed Mailbox parsing before send (header-injection mitigation)"
    - "Resend requires User-Agent; secrets never logged"

key-files:
  created:
    - crates/oxidean-api/src/email/mod.rs
    - crates/oxidean-api/src/email/log_sink.rs
    - crates/oxidean-api/src/email/smtp.rs
    - crates/oxidean-api/src/email/resend.rs
    - .planning/phases/04-auth-sessions-email/04-USER-SETUP.md
  modified:
    - crates/oxidean-api/Cargo.toml
    - crates/oxidean-api/src/lib.rs
    - Cargo.lock

key-decisions:
  - "lettre default-features disabled; rustls via tokio1-rustls + aws-lc-rs + rustls-native-certs"
  - "reqwest 0.13 uses feature rustls (not rustls-tls)"
  - "ResendSender::with_base_url for wiremock; production uses https://api.resend.com/emails"

patterns-established:
  - "Outbound mail adapters live under crates/oxidean-api/src/email/"
  - "Provider selection from ENV only in Phase 4 (secrets not in DB)"

requirements-completed: [AUTH-09, AUTH-10, AUTH-11]

duration: 3min
completed: 2026-09-09
---

# Phase 4 Plan 02: Email Adapters Summary

**EmailSender trait with LogSink (tracing), SMTP (lettre AsyncSmtpTransport), and Resend (reqwest + User-Agent) selected from ENV**

## Performance

- **Duration:** 3 min
- **Started:** 2026-09-09T23:20:34Z
- **Completed:** 2026-09-09T23:23:41Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- `EmailSender` + `OutboundEmail` + `EmailError`; default `LogSink` logs to/subject/body on `oxidean.mail` with no network (AUTH-09)
- `SmtpSender` builds/sends via lettre typed `Mailbox` addresses; invalid To → `EmailError::InvalidAddress` (AUTH-10, T-04-05)
- `ResendSender` POSTs JSON to `https://api.resend.com/emails` with Bearer auth and `User-Agent: oxidean-api/0.1`; wiremock proves headers (AUTH-11)
- `build_email_sender_from_env`: Resend key → SMTP URL → LogSink; From via `OXIDEAN_MAIL_FROM`

## Task Commits

Each task was committed atomically:

1. **Task 1: EmailSender trait + LogSink (AUTH-09)** - `4bd41c7` (feat)
2. **Task 2: SMTP + Resend adapters (AUTH-10, AUTH-11)** - `862efc4` (feat)

**Plan metadata:** `docs(04-02): complete email adapters plan` (this commit)

## Files Created/Modified

- `crates/oxidean-api/src/email/mod.rs` — trait, errors, env factory
- `crates/oxidean-api/src/email/log_sink.rs` — AUTH-09 LogSink + unit test
- `crates/oxidean-api/src/email/smtp.rs` — AUTH-10 SmtpSender + address validation test
- `crates/oxidean-api/src/email/resend.rs` — AUTH-11 ResendSender + wiremock UA test
- `crates/oxidean-api/src/lib.rs` — `pub mod email`
- `crates/oxidean-api/Cargo.toml` — lettre, reqwest, async-trait, thiserror, wiremock
- `Cargo.lock` — dependency lock updates
- `04-USER-SETUP.md` — optional SMTP/Resend operator env setup

## Decisions Made

- Disabled lettre default features to avoid native-tls/tokio1 mismatch; enabled rustls stack (`aws-lc-rs`, `rustls-native-certs`)
- reqwest 0.13 feature flag is `rustls` (plan’s `rustls-tls` name is 0.12-era)
- Transport/provider errors redact credential-shaped substrings; never log API keys or SMTP passwords (T-04-04)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] lettre TLS feature set**
- **Found during:** Task 1 (dependency compile)
- **Issue:** Default lettre features pulled `native-tls` without `tokio1-native-tls`, and rustls required crypto/certs features
- **Fix:** `default-features = false` + `tokio1,tokio1-rustls,smtp-transport,builder,hostname,aws-lc-rs,rustls-native-certs`
- **Files modified:** `crates/oxidean-api/Cargo.toml`
- **Verification:** `cargo test -p oxidean-api --lib email::` passes
- **Committed in:** `4bd41c7` (Task 1)

**2. [Rule 3 - Blocking] reqwest feature rename**
- **Found during:** Task 1 (`cargo add`)
- **Issue:** reqwest 0.13 has no `rustls-tls` feature (plan assumed 0.12 name)
- **Fix:** Use `--features rustls,json`
- **Files modified:** `crates/oxidean-api/Cargo.toml`
- **Verification:** Dependency resolves; Resend tests pass
- **Committed in:** `4bd41c7` (Task 1) / exercised in `862efc4` (Task 2)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Required for compile on current crate versions; no scope creep. Behavior matches AUTH-09/10/11.

## Issues Encountered

None beyond the dependency feature adjustments above.

## User Setup Required

**External services require manual configuration for live delivery.** See [04-USER-SETUP.md](./04-USER-SETUP.md) for:
- `OXIDEAN_SMTP_URL`, `OXIDEAN_MAIL_FROM`, `OXIDEAN_RESEND_API_KEY`
- Optional Resend account / domain verification

## Next Phase Readiness

- Email adapters ready for welcome send in **04-04** (D-20)
- No verify/reset templates introduced (D-21 / Phase 5)
- Ready for **04-03** (Argon2id + session cookies)

## Self-Check: PASSED

- `crates/oxidean-api/src/email/{mod,log_sink,smtp,resend}.rs` present
- Commits `4bd41c7`, `862efc4` in git log
- `cargo test -p oxidean-api --lib email::` — 3 passed
- Acceptance greps for `EmailSender`, `oxidean.mail`, `AsyncSmtpTransport`, `api.resend.com/emails`, `User-Agent`, `oxidean-api/0.1` — PASS

---
*Phase: 04-auth-sessions-email*
*Completed: 2026-09-09*
