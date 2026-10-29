# Feedback Loop — oxpilot

## Channels

- **User/operator reports** → GitHub issues (the forge hosts its own issue tracker — dogfood).
- **Signal mix** → refusal-code distribution (`budget_exceeded`/`flag_off`/denied), turn terminal mix, tool-round saturation, queue backlog — the log queries in `observability-setup/log-queries.md` surface each.
- **Review loop** → CodeRabbit on PRs (active on #203 now) + normal human review.

## Optimization loop

1. Measure (flag-on env): SLO actuals + `oxpilot_usage` counters.
2. Tune ceilings: transcript window, summary cap, round/timeout caps, budgets — all are constants/settings, not code paths.
3. Provider/model swap via `OXIDEAN_OXPILOT_PROVIDER`/`MODEL` — no redeploy needed.
4. Feed structural findings back as issues; fix forward on `main`.

## Non-goals

No usage telemetry is sent anywhere by the feature itself — optimization uses operator-visible signals only, consistent with the privacy posture (provider sees only what the operator's configured provider is, by their choice).
