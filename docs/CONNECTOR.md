# `ox pilot` — the oxpilot connector agent

`ox pilot` turns a local machine into an oxpilot **connector target**: it
pairs with an Oxidean forge, custodies the verifier the pair route returns,
and runs a claim/heartbeat loop that executes server-dispatched actions
inside a filesystem jail. The wire contract is **C3** — four flat-envelope
routes under `/api/oxpilot/connector/*`, *not* the `/api/rpc` envelope the
rest of the CLI uses.

## Runbook: pair → run

```bash
# 1. In the Oxidean UI: Pilot → Targets → "New pairing" — mint a code.
#    Codes live 10 minutes and are single-use.

# 2. Redeem it on the machine that should run actions:
ox pilot pair <64-hex-code> [--instance https://forge.example] \
     [--label "build-box-1"] [--caps exec_shell,fs_read,fs_write] [--verbose]

# 3. Run the connector (foreground — send ctrl-c to stop):
ox pilot run [--target <id>] [--root <dir>] \
     [--wait-seconds 30] [--exec-timeout 300] [--verbose]

# Housekeeping:
ox pilot status [--json]      # rows + live verifier probe
ox pilot disconnect [--target <id>]   # local forget
```

`pair` prints a confirmation only — the verifier is never printed, logged,
or written to stdout/`--json`. `run` needs a paired target: with exactly one
row it selects itself; with several, `--target <id>` is required.

## Jail semantics

Every action path resolves against a **canonical root** — `--root <dir>` or
the current directory at launch:

- `--root` is canonicalized once at launch; it must exist and be a directory.
- The root must not **be** the ox config dir, **contain** it, or **sit
  inside** it — checked in both directions at launch. (The default `cwd`
  root therefore fails when you run `ox pilot run` from your home directory
  or inside `~/.config/ox`; pass an explicit workspace `--root`.)
- Each action path is lexically normalized (`..` collapse → escape refused),
  then canonicalized — for `fs_write` the deepest *existing* ancestor, so
  writes can create new files under existing directories. A canonicalized
  path outside the root, or anywhere inside the config dir, is refused.
- The leaf must be a **regular file** by `symlink_metadata` — symlink,
  directory, fifo, and device paths are refused (`fs_write` may create a
  missing leaf).
- `exec_shell` runs `sh -c` (unix) / `cmd /C` (windows) with the canonical
  root as its working directory. The shell is ambient — PATH and normal env
  apply — except the environment strip below.

Named refusals (reported to the server as `failed` output):
`jail_escape:<input>` · `jail_config:<input>` · `not_regular_file:<input>`
· `not_found:<input>` · `io:<err>` · `invalid_args:<err>` ·
`capability_refused:<kind>` · `too_large:<n>` · `timeout:<n>s` ·
`spawn_error:<err>`.

## Bounds and redaction

| Bound | Value | Where |
|---|---|---|
| fs payload cap | 128 KiB **decoded** | `fs_read` output, `fs_write` input — over-cap → `failed:too_large` |
| exec output tail | 64 KiB combined | last bytes of interleaved stdout+stderr, `…<n> bytes truncated` marker |
| exec timeout | 300s default | `--exec-timeout`; kills the whole process group/tree |
| report output | < 256 KiB | universal bound — every output is far smaller by construction |
| claim wait | 30s default, clamp 1–60 | `--wait-seconds` |
| heartbeat | every 15s | carries the re-announced cancel set |

**Environment strip** (child process): `OXIDEAN_*TOKEN*`/`OXIDEAN_*KEY*`/
`OXIDEAN_*SECRET*`/`OXIDEAN_*PASS*` and any `*PAIRING*` name — case-insensitive.

**Output scrub**: every ≥8-char secret value in the scrub set is replaced
with `[redacted]` before a report is posted — the verifier, env values
under token/key/secret/pass-ish names, `config.json` instance tokens, and
`pilot-secrets.json` values. The server's own redaction still applies; the
client-side scrub is defense in depth, never the barrier.

## Exit codes

The pilot surface pins `{0,1}` — it does **not** reuse the general CLI
`2 usage`/`3 auth` outcomes for run-loop terminals:

| Exit | Causes |
|---|---|
| 0 | clean SIGINT/SIGTERM (child killed, `cancelled:<sig>` reported, creds kept) · `revoked` terminal (creds+row deleted) · claim/heartbeat `not_found` (creds+row deleted) · successful `pair`/`disconnect`/`status` |
| 1 | auth death (`unauthorized`/`forbidden` on claim/report/heartbeat — kills in-flight, deletes creds+row, no report) · `bad_input` (report-cancels, keeps creds) · pairing failure (used/expired/exceeded caps/transport) · missing row or verifier · jail-root refusal · custody unavailable · corrupt `pilot-*.json` |

Loop-survivable (never exit): `disabled`, `internal`, unknown codes,
transport failures (bounded jittered backoff `min(60, 2^n×(1±0.2))`), and
`rate_limited` (honors `Retry-After` verbatim).

## Custody model

- **Verifier** — OS keyring under `pilot/<instance>/<connector_id>` via the
  existing `keystore` service prefix; headless fallback is
  `config_dir()/pilot-secrets.json` (0600, atomic tmp→fsync→rename). Write
  order is keyring-then-file; **both** failing aborts pair before any row
  is written. Deletes always attempt both backends.
- **Identity rows** — `config_dir()/pilot-targets.json` (0600), keyed by
  `target_id`: `{instance, connector_id, target_id, device_label,
  granted_caps, requested_caps, paired_at}` — never verifier material.
- **Zombie** — a row whose verifier is gone: `status` reports
  `verifier: missing` (a live probe, not a stored field); `run` refuses
  with the named zombie message; `disconnect` is the remedy.
- **Corrupt files** — unparseable `pilot-secrets.json`/`pilot-targets.json`
  is a named parse error on every command that reads them — never a silent
  empty state.
- **Compensation** — `pair` writes the verifier first, then the row; a row
  write failure deletes the verifier so pair never creates a zombie itself.

## `disconnect` is local-forget

`ox pilot disconnect` deletes the verifier (both backends) and the target
row — **no server call**. The server-side target and connect grant stay
alive until revoked in the UI (**Pilot → Targets → revoke**). This matters
for lifecycle: revoke in the UI first if you want the forge to stop
dispatching, then disconnect locally.

## What the server can dispatch

`exec_shell` (jailed cwd), `fs_read`, `fs_write` — the closed capability
set. Anything else is refused *before* execution and reported
`failed:capability_refused:<kind>`. Actions arrive one at a time; cancels
arrive on every claim answer and heartbeat, and close out exactly once —
`cancelled:pre-start` for work never started, `cancelled:mid-flight` for a
killed process group.
