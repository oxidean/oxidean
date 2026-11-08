# Feedback & Optimization — Clarifying Questions

## Q1. How does feedback flow post-launch?

**Answer:** Operator/user reports → repo issues; oxpilot-specific signals = refusal-code mix, turn terminal mix, tool-round saturation (see observability-setup). No in-product telemetry is added — privacy posture stays intact.

## Q2. What does "optimized" mean here?

**Answer:** Prompt-window bound (128 KiB), summary caps (2 KiB), round/timeout caps — all shipped at design values; optimization = tuning these ceilings + provider/model choice post-SLO measurement, not code churn.

## Q3. Cost model?

**Answer:** Provider-token spend driven by operator-configured per-user/per-instance budgets (`oxpilot_settings`) — unbounded by default, operators opt into ceilings. No platform billing component exists or is added.

## Consolidated Summary Confirmation

- Feedback via issues + refusal/terminal signal mix; no telemetry added (Q1).
- Optimization = ceiling/model tuning after SLO measurement, not churn (Q2).
- Cost = provider tokens under operator budgets; unbounded defaults (Q3).

Does this all look correct before I write the feedback-optimization artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
