# Observability Requirements — connector-client (U7)

Consumes: functional-design `rules.md` BR1.4/BR8/BR9,
`functional-spec.md` Observability + procedure deltas, questions Q6-A,
requirements.md NFR6 (reuse existing conventions — CLI stderr/`--verbose`
idioms, not a new telemetry surface).

## Operator visibility

| id | requirement | basis |
|----|-------------|-------|
| NFR6.1 | stderr lifecycle lines on every state transition — paired (`target_id`, label, caps), each claim cycle outcome (actions received, cancel ids applied), action start/finish (action_id, kind, status), backoff entries (reason class, next delay), terminals (which code, what happened to creds) — an unattended connector must narrate its own degrade/recovery | Q6-A, NFR4.1 |
| NFR6.2 | `--verbose` on `pair`/`run` adds request/response logging: method, route, status, retry_after — with `pairing_code`, `verifier`, and `Authorization` headers REDACTED (the same secret class as BR1.4); never at INFO default, only behind the flag | functional-spec deltas, BR1.4, R-12 |
| NFR6.3 | Exit codes are the supervisor contract, documented in `--help`: `0` = clean terminal (revoked/not_found/signal/disconnect), non-zero = auth death, `bad_input`, pairing failure, missing credentials — `disabled`/transport NEVER exit (the loop stays alive so `Restart=` policies can't flap-burn) | BR8.3–BR8.5, BR4.5, BR8.2, Q6-A |

## Honest disclosure

| id | requirement | basis |
|----|-------------|-------|
| NFR6.4 | `--help` + docs pin: disconnect is LOCAL FORGET ONLY (server-side severing is the owner's `oxpilot.target_disconnect`); the fs jail's bounds and its exec caveat; the env strip; every cap and timeout constant | BR9.1, R-14, BR4.2 |
| NFR6.5 | `ox pilot status` shows metadata only (instance, connector_id, target_id, label, caps requested-vs-granted, paired_at, custody backend in use) — never verifier material even in `--json` | BR9.2 |
| NFR6.6 | Error text distinguishes operator-actionable from ambient: `disabled` names the operator ("oxpilot is disabled on this forge"), `pairing_expired`/`pairing_used` name the remedy (mint a fresh code in the UI), `forbidden` names the fix (narrow `--caps`, same code retries) | BR1.3, N-09e |

## Scope boundary

- No metrics endpoint, no tracing export, no audit emission — the
  client is a leaf process; the SERVER owns `audit_events` (U6-BR7.2)
  and the connector's server-visible lifecycle (last_seen, status).
  Client observability = stderr + exit codes + `--verbose`, riding
  existing `ox` CLI conventions (NFR6).
