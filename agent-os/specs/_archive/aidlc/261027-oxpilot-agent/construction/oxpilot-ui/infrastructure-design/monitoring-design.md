# Monitoring Design — oxpilot-ui (U8)

Consumes: `nfr-design/observability-design.md` (OBS-1–OBS-6),
`nfr-requirements/observability-requirements.md` (NFR5.1–5.2,
NFR6.1–6.4), `functional-design/rules.md` (BR9/BR11),
`nfr-design-questions.md` (Q5-A/Q7-A), `infrastructure-design-
questions.md` Q4-A.

A render surface has no monitoring infrastructure to build. The
UI owns **zero telemetry** (OBS-3/NFR6.1): no analytics, no
beacons, no console metrics, no error-reporting dependency, no
`performance.mark`/`PerformanceObserver` instrumentation. What
this unit contributes is the HONEST surface — every failure
renders a typed fix-naming notice to the user, and the coverage
manifests make dropped surfaces CI-visible. The server-side
health story is owned elsewhere and cited, not duplicated.

## User-visible signals (the unit's whole diagnostic surface)

| signal | emission | reads |
|--------|----------|-------|
| `seedError` banner | mount/reseed read-set failure | the seed failure message — retry in place (REL-6) |
| `es.onerror` → typed fatal | `readyState` CLOSED ⇒ one `sessionGet` probe | `not_found` ⇒ notice + back link; `oxpilot.disabled` ⇒ flag-off swap; other ⇒ generic stream-error banner — never a dead spinner (REL-3) |
| composer banners | per-code proc denials | `turn_active` ⇒ the queued path; `budget_exceeded` ⇒ names `OXIDEAN_OXPILOT_*` env vars + `docs/CONFIGURATION.md`; `session_active_turn_absent`, `forbidden`/`not_found`, `bad_input` field message (OBS-4) |
| transcript notices | `terminal{error}` / `terminal{aborted}` / queued rows | "The provider stopped mid-reply — your messages are saved." + `[Retry]`; "Reply stopped"; queued-row states (REL-12) |
| card outcome lines | frozen `grantGet`-resolved states | `allowed`/`denied`/`expired`/`revoked`/`cancelled — target offline`; `diff_not_pending`/`apply_failed:*` banners (REL-7/REL-10) |
| flag-off `EmptyState` | `oxpilot.status` `enabled===false` or mid-session `oxpilot.disabled` | reason (`flag_off`/`misconfigured`/`no_provider`), the two env names, CONFIGURATION.md link — ZERO controls (SEC-9, NFR8.1) |
| mutation toasts | `toastSuccess` ONE per CONFIRMED mutation | PR created + `pr_ref` link; target connected/disconnected/revoked; session deleted — never on optimistic state (OBS-2) |

Every line is textual and names the remedy — never a bare code,
never a spinner that outlives its failure (BR9.1/9.2).

## Mutation record — attribution rides the server (NFR5.1)

Every user action maps to exactly one C1 proc the server audits:
`sessionCreate`/`sessionDelete`, `pairingCreate`, `grantRespond`,
`targetDisconnect`/`targetRevoke`, `diffApply`, `messageQueue`,
`turnCancel`, `POST …/chat`. The UI emits no client-side audit
vocabulary, fabricates no state, and holds no optimistic copy
that outlives its proc — [domain] "the UI enforces nothing
itself" is the observability posture too.

## Coverage manifests as behavior observability (NFR6.3)

The `route-coverage` happy-dom rows + the subject-keyed
`browser-coverage` rows on the two discovered surfaces
(`oxpilot-new-session` + `oxpilot-repo-trigger` on
`new-session-dialog.tsrx`; `oxpilot-connect` on
`connect-dialog.tsrx`) pin each high-risk surface in CI — the
button-only subjects (consent, rail collapse, diff card cluster,
session/target rows, drawer) are exercised inside the kept
happy-dom route rows, since the gate orphan-fails browser rows
on undiscovered files (`browser-coverage-check.ts:255-259`) — a
dropped row or dropped subject is a CI-visible regression,
never a silent gap (OBS-5/BR11). The structural grep pins ride the same lanes:
the SEC-2 two-sink `dangerouslySetInnerHTML`/`innerHTML`-class
allowlist, `pairing_code` confinement to `connect-dialog.tsrx` +
the `CopyCommand` call site, and the verbatim `type:"tool_call"`
discriminant — all asserted inside `oxpilot.integration.test.ts`/
`oxpilot-stream.unit.test.ts`.

## What the SERVER owns (cited, not duplicated)

| concern | owner surface |
|---------|---------------|
| stream/turn health — turn spans, queue drain, terminal vocabulary, claim-loss warns | U3 `agent-runtime/infrastructure-design/monitoring-design.md` — `tracing` span table |
| grant/target/connector lifecycle — consent audits, `last_seen_at` liveness, claim park/wake | U6 `connector-gateway/infrastructure-design/monitoring-design.md` — operator signals + audit rows |
| `oxpilot.*` audit vocabulary (`proposal_created`, `diff_applied`, consent/target lifecycle) | U5 `oxpilot-tools/infrastructure-design/monitoring-design.md` + U6's audit surface |
| enablement health — gate-denial reasons, `oxpilot.status` as operator probe | U1 `skeleton-slice/infrastructure-design/monitoring-design.md` — tracing + status surface |
| connector-side liveness signals | U7 `connector-client/infrastructure-design/monitoring-design.md` — stderr/exit/status |

## What is NOT monitored client-side (by design)

- No telemetry pipeline of any kind — NFR6.1's pin is absolute:
  an error-reporting dep would also be an egress path for
  transcript content SEC-15/SEC-16 refuse.
- No perf instrumentation — the NFR3 typing-echo proxy is held
  structurally (append-only render, boundary announcements,
  bottom-only autoscroll); the happy-dom identity-preservation
  assertion is the CI leg, not a runtime probe (Q5-A).
- No console-only failure paths — console output is never the
  sole record of anything; the pairing code never reaches
  `console.*` at all (SEC-14), and cleanup-time swallows
  (reader cancels, clipboard fallback) are noise, not failures
  (OBS-6).
- No health endpoints, metrics exporters, dashboards, or alert
  rules — the repo ships log observability server-side; the
  browser surface has nothing to export to.
