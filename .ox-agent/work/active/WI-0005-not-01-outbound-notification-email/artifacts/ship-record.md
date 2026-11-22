# WI-0005 ship record — NOT-01 outbound notification email

- **Branch:** `feat/not-01-outbound-email` (pushed to origin)
- **PR:** https://github.com/oxidean/oxidean/pull/285 — `feat(api-notification): outbound notification email for issue and PR activity`
- **Base freshness:** merge-base(HEAD, origin/main) = origin/main tip `62b9ffea`; `git merge origin/main` → "Already up to date."
- **Commits (5):**
  - `dba765f5` feat(db): batch primary-verified email lookup
  - `716ed995` feat(api-core): email notification fan-out recipients
  - `51f7674a` docs: document OXIDEAN_NOTIFY_EMAIL_ENABLED
  - `6e0ae125` chore: WI-0005 pipeline artifacts — spec, plan, ADR
  - `b5de5123` chore: WI-0005 pipeline state — verification green, ship-ready
- **Review requested:** `@coderabbitai review` comment posted on PR #285.
- **CI at ship time:** compose / meta / makefile-lint / ci-image passing; remaining lanes pending.
- **Human gate:** `shipping → shipped` requires human per lifecycle; do-not-merge honored.
- **Rollback:** revert the 5 commits — no migrations, no wire changes; `OXIDEAN_NOTIFY_EMAIL_ENABLED=0` restores inbox-only behavior without a revert.
