# Cost Analysis — oxpilot

## Cost drivers

- **Provider tokens** — the only marginal cost; governed by `oxpilot_settings` per-user + per-instance budgets (daily counters; unbounded defaults; refusal-before-spend at turn start).
- **Platform infra** — zero new services; rides existing api/web/postgres. Marginal storage = oxpilot tables (messages/tool-calls/usage) — bounded by transcript caps + streaming-message sweep.
- **Egress** — connector protocol is operator-configured; SSE is modest-volume (snapshot+append frames, deduped client-side).

## Controls shipped

Dual-axis daily budget counters recorded at turn terminal; `budget_exceeded` refusal before enqueue/claim; tool summaries capped so replay cost stays bounded; single-turn-per-session concurrency.

## Recommendation

Operators enabling the flag should set per-instance ceilings first, observe `oxpilot_usage` counters for a week, then tune per-user ceilings — data exists in `usage_get` (shipped for this purpose).
