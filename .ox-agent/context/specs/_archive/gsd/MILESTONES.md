# Milestones

## v1.0 MVP (Shipped: 2026-09-19)

**Phases completed:** 24 phases (1–22 + 11.1 + 22.1), 220 plans, 323 tasks  
**Closeout:** verified_closeout (fingerprint refresh; tests-as-proof)  
**Git range:** scaffold (2026-09-09) → archive (2026-09-19) · ~2057 files · ~10 days  
**Requirements:** 87/87 v1 Complete

**Key accomplishments:**

- Shipped a dual-mode GitHub-shaped forge: Rust Axum RPC + Octane TanStack Start UI, multi-DB, Compose CI, Railway-class deploy path
- Auth end-to-end: sessions, email verify, password reset, self-host admin bootstrap / factory reset
- Git hosting: browse, HTTPS PATs, SSH, LFS, releases, rename/transfer, in-repo search
- Collaboration: orgs/ACL, issues, PRs (review + merge strategies), branch protection including packaged direct-push denial
- Platform surface: Actions-compatible runners, OCI/npm/generic packages, notifications, webhooks, stars/explore/forks
- Quality: Phase 11.1 chrome/coverage/e2e hardening; Phase 22.1 ORG-06 packaging + VERIFICATION backfill

**Archives:**

- [v1.0-ROADMAP.md](milestones/v1.0-ROADMAP.md)
- [v1.0-REQUIREMENTS.md](milestones/v1.0-REQUIREMENTS.md)
- [v1.0-MILESTONE-AUDIT.md](milestones/v1.0-MILESTONE-AUDIT.md)
- [v1.0-phases/](milestones/v1.0-phases/)

**Known residual debt (not blockers):**

- CI omits `smoke-protection` (ORG-06 regression guard)
- Nyquist partial on several mid phases; Class C deferred (GlobalSearch depth, issue_comment webhook, Checks tab, follow/watch, live Railway apply)
