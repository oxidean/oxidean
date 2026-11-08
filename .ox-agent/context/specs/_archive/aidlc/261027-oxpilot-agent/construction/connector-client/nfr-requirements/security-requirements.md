# Security Requirements — connector-client (U7)

Consumes: `construction/connector-client/functional-design/rules.md`
(BR1–BR9), `functional-design/entities.md`,
`functional-design/functional-spec.md`, `nfr-requirements-questions.md`
(Q1–Q7 answered A),
`inception/requirements-analysis/requirements.md` (NFR1, NFR2),
`inception/contract-design/contract-summary.md` (C3),
`codekb/oxidean-app/technology-stack.md`.

The client holds the only usable verifier material in the connector
plane (the server stores only its hash) and is the last user-controlled
surface before report output reaches the forge's transcript store and
the provider's context — every pin below is client-side enforcement,
never operator policy.

## Credential custody

| id | requirement | basis |
|----|-------------|-------|
| NFR1.1 | Verifier material lives ONLY in the two-backend seam: `keystore::set("pilot/<instance>/<connector_id>", verifier)` first; on `set` Err (headless — no Secret Service) a defined `pilot-secrets.json` map `{ "<instance>/<connector_id>": "<verifier>" }` mode 0600 under `config_dir()`. Pairing aborts iff BOTH backends fail; read order mirrors write order | BR2.4, BR1.2, Q2-A |
| NFR1.2 | Verifier is NEVER written to `pilot-targets.json`, never to `config.json`, never printed/logged/serialized — `status --json` omits it structurally; `--verbose` request logs redact `Authorization`/`verifier`/`pairing_code` (BR1.4's class); it never enters a child process env (BR4.2) | BR1.4, BR4.2, BR9.2, Q2-A |
| NFR1.3 | Custody deletion is pinned to every terminal path: `disconnect` (BR9.1), BR8.4 terminals (heartbeat `revoked:true`, `revoked` on claim/report, `not_found` on claim/heartbeat), BR8.3 auth death (`unauthorized`, off-pair `forbidden`). `disabled`/`rate_limited`/`bad_input`/transport NEVER delete — transient server states must not burn the credential fleet | BR8.3, BR8.4, BR2.3, Q4-A |
| NFR1.4 | A `pilot-targets.json` row whose verifier is absent from BOTH backends is a named zombie: `run` reports `credentials missing for <target_id> — re-pair or ox pilot disconnect --target <id>` and exits non-zero, `status` displays the zombie state; the row is PRESERVED under read paths (the server-side grant may be live — an unprompted local delete would strand it) — BUT `disconnect --target <id>` still deletes the identity row: it IS the named remedy, and the custody-backend deletes are no-ops on a zombie (BR9.1's local-forget never depends on the verifier being present) | BR9.3, BR9.1, NFR4 |

## Execution boundary

| id | requirement | basis |
|----|-------------|-------|
| NFR1.5 | fs_* resolves `root/path` lexically: `.`/`..` normalize then escape refusal, symlink realpath recheck (a `root/link → /etc` escape is refused) — the jail covers fs_* deterministically; `exec_shell` runs `cwd=root` but the SHELL is not path-jailed (documented) | BR6.1, BR4.2 |
| NFR1.6 | Hard exclusion regardless of `--root`: `config_dir()` and everything under it — `pilot-targets.json`, `pilot-secrets.json`, `config.json` (user PATs under `instances.*.token` via the keystore file fallback), the OS keystore file if it lives there — refused like an escape (`failed:jail_config`); stored Oxidean secrets never reach report output by ANY path (FR5.4) | BR6.3, R-05 |
| NFR1.7 | Non-regular files refused at resolve for read AND write (`failed:not_regular_file`): FIFOs/devices/sockets can block forever and are uncancellable — the during-execution cancel MUST can't be honored on them | BR4.3, BR4.4, R-09 |
| NFR1.8 | Child env drops the verifier and `OXIDEAN_*TOKEN*`, `OXIDEAN_*KEY*`, `OXIDEAN_*SECRET*`, `OXIDEAN_*PASS*`, `*PAIRING*` vars — the OXIDEAN_ prefix scope is pinned so the child's OWN credentials (e.g. `AWS_SECRET_ACCESS_KEY`) survive | BR4.2, N-07c |
| NFR1.9 | Report `output` is value-scrubbed before send with the enumerated set the client can itself produce: verifier, env values matching `*TOKEN*`/`*KEY*`/`*SECRET*`/`*PASS*` (≥8 chars), `config.json` `instances.*.token` values, `pilot-secrets.json` values → `[redacted]`; catches the exec `cat config.json` residual. The server redactor is the SECOND pass (FR5.5), not the only one | BR7.2, R-05, Q3-A |

## Protocol honesty

| id | requirement | basis |
|----|-------------|-------|
| NFR1.10 | claim/report/heartbeat authenticate ONLY with `Authorization: Bearer <verifier>` — never `resolve_token`, never a user PAT, never a session cookie; `/pair` is the sole unauthenticated call and its pairing code is itself single-use credential material (never written to disk, redacted in `--verbose`) | BR2.1, BR1.4, C3 |
| NFR1.11 | Wire errors key on the flat `{"error":"<code>"}` CODE — never HTTP status alone (several codes share one status); the full vocabulary is handled: `unauthorized`, `forbidden`, `revoked`, `disabled`, `not_found`, `bad_input`, `internal`, `rate_limited`, `pairing_expired`, `pairing_used`; `openapi.yaml` enumerates all ten | BR2.3, BR1.3, ConnectorError |
| NFR1.12 | Caps-violating claimed action kinds are refused + reported `failed`, never executed — the client re-enforces its own bound even against a misbehaving/compromised dispatch (`min(requested, granted)` recorded at pair, sent as `claim.capabilities`) | BR3.2, BR1.1, AC4.6.1 |

## Threat considerations

- A leaked `pilot-targets.json` buys nothing — metadata only, the
  verifier lives elsewhere (Q3-A custody split).
- A stolen verifier authenticates until the owner revokes the target
  in the UI (disconnect is local-forget — BR9.1 documents this
  plainly); revocation reaches the client via heartbeat/`revoked`
  error and triggers local deletion.
- `exec_shell`'s cwd jail does not confine the shell — a user-granted
  exec capability is ambient shell access on their own machine;
  disclosed in `--help`/docs, mitigated for Oxidean secrets by the
  config-dir exclusion + BR7.2 value scrub.
