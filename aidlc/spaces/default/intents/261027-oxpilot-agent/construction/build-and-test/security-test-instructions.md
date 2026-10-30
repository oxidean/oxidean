# Security Test Instructions — oxpilot

NFR security targets (NFR1.x/NFR2.x, SEC pins) with executable checks:

| Target | Check |
|---|---|
| Cross-user session isolation | `cargo nextest run -p oxidean-api --test oxpilot` — `session_create_list_and_cross_user_isolation` |
| PAT credential denied session-scoped | `pat_credential_denied_session_only` in same suite |
| SSE routes deny when flag disabled | `sse_routes_deny_when_disabled` |
| Consent-gated computer use | `cargo test -p oxidean-api-oxpilot --test oxpilot_connector` + `cargo test -p oxidean-cli --test pilot_connector` — grant list/revoke + connector authn |
| Owner propagation / authz on every proc | oxpilot.rs integration suite — per-proc denial tests |
| No provider creds/content in logs | `cargo test -p oxidean-core redact` — redaction tests; SEC pin in UI tests (`contentHtml` raw-sink regex) |
| Tool summaries bounded (leak surface) | summarizer cap tests (`≤ 2 KiB`) |
| `budget_exceeded` env-naming refusal | UI budget-notice test (no secret echo) |

## Static checks

```bash
make web-lint            # type-aware oxlint incl. DOM-race heuristic
make dead-code-check     # clippy -D dead_code + knip
grep -rn "innerHTML\|contentHtml" apps/web/src --include='*.ts*'   # raw-sink audit — highlight-diff only, tested
```

## DAST / deployed-environment checks

Consent handshake, token rotation, and remote-target TLS posture against a running stack are **owned by operation-phase security/validation stages**; the e2e-stack suite covers the flag-off denial path locally/in CI.
