# Observability Requirements — oxpilot-ui (U8)

Consumes: functional-design `rules.md` BR9/BR11,
`functional-spec.md` (WF12, Observability section),
`frontend-components.md` (API integration points), questions Q8-A,
`inception/requirements-analysis/requirements.md` NFR5 (mutations
attributable to the user) + NFR6 (reuse existing surfaces — no new
observability stack) + NFR8 (errors name the fix), shipped
`lib/toast.ts`.

## Mutation record — the UI's auditability half

| id | requirement | basis |
|----|-------------|-------|
| NFR5.1 | The UI's auditability contribution is honesty, not rows: every mutation rides a real C1 proc the server audits/attributes (`session_create`/`session_delete`, `target_disconnect`/`target_revoke`, `pairing_create`, `grant_respond`, `diff_apply`, `message_queue`, `turn_cancel`) — the UI NEVER fabricates a state, emits no client-side audit/event vocabulary of its own, and holds no optimistic copy that outlives its proc | spec Observability, C1, [domain] |
| NFR5.2 | Mutation success toasts are the user-visible completion record — PR created (toast + `pr_ref` link), target connected/revoked/disconnected, session deleted — via `toast.ts`, ONE toast per confirmed mutation, never on optimistic state (a failed `diff_apply` keeps the card pending and toasts nothing) | spec Observability, WF9, WF10.4 |

## Diagnostic surfaces — no telemetry

| id | requirement | basis |
|----|-------------|-------|
| NFR6.1 | ZERO UI-side telemetry in v1: no analytics, no beacons, no console metrics, no error-reporting dependency — server audit (U5/U6) owns who/what/when; this unit reuses existing surfaces only | spec Observability, NFR6 |
| NFR6.2 | Typed-error surfacing IS the diagnostic trail: every failure renders a textual notice naming the outcome — `seedError` banner, `es.onerror` probe → typed banner naming `not_found`/`oxpilot.disabled`, per-code composer banners, `budget_exceeded` naming the `OXIDEAN_OXPILOT_*` env vars + CONFIGURATION.md (NFR8's "errors name the fix" lands here) | WF12, BR9.1/BR9.3, AC5.2.2 |
| NFR6.3 | Coverage manifests are the behavior observability: the `route-coverage` happy-dom rows + subject-keyed `browser-coverage` rows pin each high-risk surface (browser rows only on gate-discovered surfaces — new-session dialog, connect dialog; button-only surfaces ride the happy-dom route rows — the gate orphan-fails browser rows on undiscovered files) — a dropped row is a CI-visible regression, never a silent gap | BR11.1/BR11.2, NFR9 |
| NFR6.4 | No console-only error paths: every failure that matters renders to the user (banner/notice/outcome line) — console output is never the sole record of a failure | BR9.1, WF12 |

## Scope boundary

- The `audit_events` vocabulary (`oxpilot.*` action/consent/target
  lifecycle rows) belongs to U5/U6; transcript-content audit and
  redaction bind U2/U3 — this unit emits nothing on `audit_events`
  and adds no observability stack (NFR6.1).
- Metrics stay on the route/proc surfaces the repo already carries;
  the UI contributes honest surfaces (toasts, typed banners,
  coverage rows), not measurements.
