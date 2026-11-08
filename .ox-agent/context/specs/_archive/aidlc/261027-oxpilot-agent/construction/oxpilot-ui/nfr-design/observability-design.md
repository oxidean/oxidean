# Observability Design — oxpilot-ui (U8)

Consumes: `nfr-requirements/observability-requirements.md`
(NFR5.1–5.2 audit; NFR6.1–6.4 telemetry), `functional-design/
rules.md` (BR9/BR11), `functional-spec.md` (WF12, Observability),
`frontend-components.md` (API integration points), `nfr-design-
questions.md` (Q7-A), `requirements.md` NFR5 (mutations
attributable to the user) + NFR6 (reuse existing surfaces — no new
observability stack) + NFR8 (errors name the fix), shipped
`lib/toast.ts` (`appToastManager` + `toastSuccess`/`toastError`),
both coverage manifests.

Observability here is deliberately thin: the UI owns ZERO
telemetry and ZERO audit rows — server audit (U5/U6) owns
who/what/when. This unit's contribution is the honest surface:
every mutation rides a real server-audited proc, confirmed
mutations toast, every failure renders a typed fix-naming notice,
and the coverage manifests make surface regressions CI-visible.

## Mutation record — honesty, not rows (NFR5.1, OBS-1)

Every user action maps to exactly one C1 proc the server
audits/attributes — the UI NEVER fabricates a state, emits no
client-side audit vocabulary, and holds no optimistic copy that
outlives its proc:

| UI action | proc | server-visible record |
|---|---|---|
| New session `[Start]` | `sessionCreate` | session row |
| Delete session confirm | `sessionDelete` | delete audit |
| Send (idle) | `POST …/chat` | transcript rows + turn |
| Queue (live turn) | `messageQueue` | queue row + `card{queued}` |
| `[Stop]` own turn | `turnCancel` | `terminal{result:aborted}` |
| Connect step 1 | `pairingCreate` | pairing row (TTL) |
| Consent buttons | `grantRespond` | grant state + audit |
| Manage `[Allow]`/`[Deny]` | `grantRespond` | grant state + audit |
| Disconnect/Revoke confirms | `targetDisconnect`/`targetRevoke` | lifecycle + audit |
| `[Create branch + PR]` confirm | `diffApply` | `pr_ref` + apply audit (U5) |

[domain]'s "the UI enforces nothing itself" is the observability
posture too: attribution lives on the server rows these procs
write; the UI adds none of its own.

## Toast discipline (NFR5.2, OBS-2)

Success toasts are the user-visible completion record — via
`lib/toast.ts` (`toastSuccess`), ONE per CONFIRMED mutation,
never on optimistic state:

| event | toast |
|---|---|
| `diffApply` ⇒ `{pr_ref}` | PR created + `pr_ref` `AppLink` (WF10.4) |
| `targetDisconnect`/`targetRevoke` | target disconnected/revoked |
| `sessionDelete` | session deleted |
| connect flow completes | target connected |

A failed `diffApply` (`diff_not_pending`/`apply_failed:*`) keeps
the card pending and toasts NOTHING — the typed banner is the
record (NFR4.6). Failure never produces a success toast; toasts
are transient by design and never the sole record of anything
(they complement, never replace, the persisted row).

## Diagnostic surfaces — zero telemetry (NFR6.1, OBS-3)

NO UI-side telemetry in v1: no analytics, no beacons, no console
metrics, no error-reporting dependency, no `performance.mark`/
`PerformanceObserver` instrumentation (Q5-A). Server audit owns
who/what/when; this unit reuses existing surfaces only and adds
no dependency whose purpose is measurement.

## Typed-error surfacing IS the diagnostic trail (NFR6.2/NFR8.2, OBS-4)

| surface | carries |
|---|---|
| `seedError` banner | the seed failure message |
| `es.onerror` probe → fatal | typed banner naming `not_found` / `oxpilot.disabled` |
| composer banners | per-code text — `turn_active` (the queued path), `budget_exceeded` (the `OXIDEAN_OXPILOT_*` env vars + `docs/CONFIGURATION.md` — AC5.2.2's "name the fix"), `session_active_turn_absent`, `forbidden`/`not_found`, `bad_input` field message |
| transcript notices | `terminal{error}` ("The provider stopped mid-reply — your messages are saved." + `[Retry]`), `terminal{aborted}` ("Reply stopped"), queued-row states |
| card states | frozen outcome lines, `diff_not_pending`/`apply_failed:*` banners |
| flag-off (NFR8.1's copy) | reason (`flag_off`/`misconfigured`/`no_provider`), `OXIDEAN_OXPILOT_ENABLED`, `OXIDEAN_OXPILOT_PROVIDER`, CONFIGURATION.md link |

Every notice is textual and names the remedy — never a bare code,
never a dead spinner (BR9.1/9.2, WF12).

## Coverage manifests as behavior observability (NFR6.3, OBS-5)

The `route-coverage.manifest.ts` happy-dom rows +
subject-keyed `browser-coverage.manifest.ts` rows pin each
high-risk surface — browser rows on the two discovered surfaces
(new-session dialog, connect dialog two-step — incl. the
`oxpilot-repo-trigger` surface rebind); the button-only surfaces
(consent card, rail collapse, diff card cluster, session/target
row actions, drawer toggle) ride the happy-dom route rows since
the gate orphan-fails browser rows on undiscovered files
(`browser-coverage-check.ts:255-259`; Q7-A, narrowed at
infra-design review). A dropped row or a dropped subject is a
CI-visible regression — the manifests are the behavioral
observability surface: they fail loudly where a silent UI gap
would otherwise ship (BR11.1/11.2, NFR9).

## No console-only error paths (NFR6.4, OBS-6)

Every failure that matters renders to the user — banner, inline
notice, outcome line, or flag-off swap. Console output is never
the sole record of a failure: the pairing code never reaches
`console.*` at all (SEC-14), stream/seed failures land on
banners, and swallowed-async paths (reader cancels, the
`catch {}` on clipboard fallback) are cleanup noise, not failure
records.

## Scope boundary

- The `audit_events` vocabulary (`oxpilot.*` action/consent/target
  lifecycle rows) belongs to U5/U6; transcript-content audit and
  redaction bind U2/U3 — this unit emits nothing on
  `audit_events` and adds no observability stack (NFR6.1).
- Metrics stay on the route/proc surfaces the repo already
  carries; the UI contributes honest surfaces — toasts, typed
  banners, coverage rows — not measurements.
