# Observability Design — connector-client (U7)

Consumes: `nfr-requirements/observability-requirements.md`
(NFR6.1–6.6), `functional-design/rules.md` BR1.4/BR8/BR9,
`functional-spec.md` Observability, `nfr-design-questions.md` Q4-A/Q7-A.

## Lifecycle narration (NFR6.1)

stderr `eprintln!` lines on transitions — one line per event,
`<verb> <subject> <detail>` shape riding `ox`'s CLI conventions:

```text
paired target=<id> label=<l> caps=<set> backend=<keyring|file>
claimed 2 action(s), cancel set {id,id}
action <id> exec_shell — done exit=0 out=3.1KiB
action <id> cancelled:mid-flight (server)
backoff transport — retry in 14s (streak 3)
terminal revoked — credentials deleted, exit 0
```

Backoff entries always name the class (transport/disabled/
rate_limited) + the delay so an operator sees the degrade reason at a
glance. No line ever carries verifier material, pairing codes, args
payloads, or raw output — ids/kinds/sizes/codes only.

## `--verbose` request log (NFR6.2)

Behind the per-command flag on `pair`/`run` only: `→ POST /api/
oxpilot/connector/claim wait=30` / `← 200 actions=2 cancel=0 retry=0`.
Header dump redacts `Authorization` (always), `pairing_code` (pair
body), `verifier`/`token` fields in response bodies — the redact list
is the same struct used by the output scrub, so a miss can't leak
through a second path.

## Exit-code contract (NFR6.3)

```text
0    clean terminal: BR8.4 revoked/not_found, signal close-out,
     successful disconnect
1    auth death, bad_input, pairing failure (expired/used/forbidden),
     missing credentials, jail-refused --root, custody-unavailable,
     corrupt pilot-targets.json, corrupt pilot-secrets.json
(never)  disabled / rate_limited / transport / internal — loop lives
```

The contract is printed verbatim in `ox pilot run --help` so a
supervisor author sees it without reading source (Q6-A).

Deliberate divergence from `CliError`'s `EXIT_AUTH=3`/`Usage=2`
taxonomy: those constants model interactive-command failures (bad
credentials mid-command, bad flags), while a connector's exit code is
a supervisor contract — 0 means "the credential was cleanly retired,
don't restart," 1 means "human attention needed." Collapsing every
failure class onto one non-zero keeps the signal that matters crisp
(would a restart help? never vs. maybe) and is called out here
because upstream only pins "non-zero."

## Honest disclosure (NFR6.4–6.6)

- `disconnect --help` leads with LOCAL FORGET ONLY + the UI-revoke
  pointer; `run --help` carries the jail semantics (fs_* jailed,
  exec cwd-jailed + ambient-shell caveat), the env strip, and every
  constant (caps/timeouts) by name.
- `status` prints rows as a table: instance, connector_id, target_id,
  label, requested→granted caps, paired_at, custody backend,
  `verifier: present|missing` — `--json` emits the same fields
  (secret material structurally absent).
- Error text is operator-actionable per BR1.3: `disabled` → "oxpilot
  is disabled on this forge — ask the operator"; `pairing_expired` →
  "code expired — mint a fresh one in the UI"; `forbidden` → "caps
  beyond the minted bound — narrow --caps and retry (same code)";
  zombie → "credentials missing for <id> — re-pair or
  ox pilot disconnect --target <id>".

## Scope boundary

No metrics endpoint, no tracing subscriber beyond `ox`'s existing
`-v` convention, no audit emission — the server owns `audit_events`
(U6-BR7.2) and the connector's server-visible lifecycle (last_seen,
status); the client's whole observability surface is stderr + exit
codes + `--verbose` + `status`.
