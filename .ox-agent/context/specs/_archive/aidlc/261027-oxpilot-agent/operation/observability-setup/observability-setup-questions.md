# Observability Setup — Clarifying Questions

## Q1. What observability surface does oxpilot use?

**Answer:** The project's existing logging/metrics surfaces only (NFR6) — Railway service logs + existing API logging; no new metrics infra, dashboards service, or tracing backend is added by this intent.

## Q2. What must never be observed?

**Answer:** Provider credentials, message content, and tool outputs are never logged — `oxidean-core` redaction is the enforcement seam (NFR2.x); operator-visible fields are ids, counts, latencies, statuses.

## Q3. Which SLOs are defined?

**Answer:** First-SSE-frame <250ms p99 and `session_list` <150ms p99 (session-core NFR3.x) — measured post-deploy in `performance-validation`; no live instrumentation exists pre-deploy.

## Consolidated Summary Confirmation

- Reuse existing log/metrics surfaces; no new observability infra (Q1).
- Redaction boundary: creds/content never logged; ids/counts/latency only (Q2).
- Two p99 SLOs owned by operation-phase performance-validation (Q3).

Does this all look correct before I write the observability-setup artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
