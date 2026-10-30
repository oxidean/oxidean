# Monitoring Design — connector-client (U7)

Consumes: `nfr-design/observability-design.md` (NFR6.1–6.6 pins,
exit-code contract, narration lines), `nfr-requirements/
observability-requirements.md`, `functional-design/rules.md`
(BR1.3/BR8/BR9), `infrastructure-design-questions.md` Q4-A.

The connector's monitoring surface is deliberately thin: it is a
user-side process whose operator is the user who launched it. There
is no telemetry channel, no metrics exporter, and no server-side
signal to build — its server-visible liveness is U6's already-built
`last_seen_at` + 90s heartbeat window + `oxpilot.target_*` audit
rows. This file names the signals the client itself emits.

## Operator signals (stderr, `eprintln!`)

| signal | emission | reads |
|--------|----------|-------|
| pairing success | `paired target=<id> label=<l> caps=<set> backend=<keyring\|file>` | custody path in use; backend named without secret bytes |
| claim batch | `claimed <n> action(s), cancel set {ids}` | dispatch throughput |
| action lifecycle | `action <id> <kind> — done exit=<n> out=<size>` / `cancelled:mid-flight (server)` / `cancelled:pre-start` | per-action outcome; sizes/ids only — never args, output, or env values |
| degrade | `backoff <class> — retry in <n>s (streak <k>)` | transport/disabled/rate_limited classes named so a flap's cause is visible at a glance |
| terminal | `terminal <reason> — credentials deleted, exit 0` / auth-death variant exit 1 | why the loop ended |
| custody event | `verifier deleted (keyring+file)` / `credentials missing for <target_id>` | zombie + delete-path confirmation |

Every line is bounded ids/kinds/sizes/codes — NFR6.x and BR7.2
forbid verifier material, pairing codes, args payloads, env values,
and raw output on any client surface.

## `--verbose` request log

`ox pilot pair|run --verbose` adds the wire shape: `→ POST
/api/oxpilot/connector/<route> <key fields>` / `← <status>
<shape>`. The redact set is the SAME enumerated-value list the
output scrub uses — `Authorization`, `pairing_code`, `verifier`,
`token` fields are always masked, so the debug surface cannot leak
through a second path.

## `ox pilot status` — the standing view

Prints every `pilot-targets.json` row (all rows, no `--target`
gate): instance, connector_id, target_id, label, requested→granted
caps, paired_at, custody backend (from the live probe), `verifier:
present|missing`. `--json` emits identical fields; secret material
is structurally absent (the row schema never holds it).

## Exit-code contract (supervisor interface)

```text
0    BR8.4 clean terminals (revoked/not_found), signal close-out,
     successful disconnect
1    auth death, bad_input, pairing failure, missing credentials,
     jail-refused --root, custody-unavailable, corrupt
     pilot-targets.json, corrupt pilot-secrets.json
(never)  disabled / rate_limited / transport / internal — loop lives
```

Printed verbatim in `ox pilot run --help`. Deliberate divergence
from `CliError`'s EXIT_AUTH=3/Usage=2 is documented in
observability-design.md — the connector's code is a restart-or-not
contract for wrappers, not the interactive taxonomy.

## What is NOT monitored client-side (by design)

- No latency/throughput metrics — the request budget is ~5–6/min
  fixed; there is nothing to graph.
- No crash reporting — a crashed connector's server-side row reads
  offline within ~90s (U6's `last_seen_at` window); the UI owner
  sees the truth there.
- No log rotation/ship — stderr is the supervisor's job to capture.
