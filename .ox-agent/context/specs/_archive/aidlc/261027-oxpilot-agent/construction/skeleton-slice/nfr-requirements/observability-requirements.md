# Observability Requirements — u1-skeleton-slice

Consumes: `functional-design/rules.md` (BR2.3, BR4.x),
`inception/requirements-analysis/requirements.md` (NFR5, NFR6),
`codekb/oxidean-app/technology-stack.md` (tracing, audit.rs).

## Logging

| id | requirement | basis |
|----|-------------|-------|
| NFR6.1 | Reuse `tracing` only — no new logging framework, metrics exporter, or dashboard | NFR6 |
| NFR6.2 | Emit `oxpilot`-targeted span/log events at: gate denial (proc/route, reason — never env values), turn claim/409, terminal result, orphan sweep count, session create (delete is U2 — R-01) | Q5 |
| NFR6.3 | Log content is metadata only — ids, codes, counts; message content and any credential-shaped value never enter a log line | NFR1.9 |

## Audit events

| id | requirement | basis |
|----|-------------|-------|
| NFR5.1 | `oxpilot_session.create` emits an `audit_events` row fire-and-forget via existing `audit.rs` — audit failure never fails the operation. Session delete is NOT a U1 surface (the skeleton ships exactly `{status, session_create, session_list}` — BR1.1); delete's audit row lands with the U2 delete proc | NFR5 share, Q4, R-01 finding |
| NFR5.2 | Audit `detail` carries `{session_id, repo_owner, repo_name, branch}` for create — never message content or secrets | NFR1.9 |
| NFR5.3 | Cross-user `not_found` denials MAY emit a fire-and-forget security audit (no content) — optional, matching the existing security-relevant convention | BR3.3 violation note |

## SLI probes (manual in v1)

| id | requirement | basis |
|----|-------------|-------|
| NFR6.4 | The stub round-trip is self-verifying in a stack test: POST → frames → terminal — no synthetic probe infrastructure in U1 | NFR9 |
| NFR6.5 | `oxpilot.status` response is the operator's diagnostic surface — `{enabled, provider, reason}` answers "why is it off" without log access | BR2.3, BR5.4 |
