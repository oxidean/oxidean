# Infrastructure Specification — connector-client (U7)

Consumes: `nfr-design/*`, `nfr-requirements/*`,
`functional-design/{rules,entities,functional-spec}.md`,
`inception/domain-design/components.md` (`OxpilotConnector`),
`infrastructure-design-questions.md` (Q1–Q7 answered A),
`team.md` (Testing Posture, Deployment, Code Style).

## Deployable surface

U7 adds **no deployable, no sidecar, no listener**. The connector
runs on the USER's machine as verbs inside the shipped `ox` binary
(`crates/oxidean-cli`). What ships:

| surface | change | infra impact |
|---------|--------|--------------|
| `crates/oxidean-cli` | `src/pilot/{mod,client,custody,exec,state}.rs` + `src/pilot/openapi.yaml` (checked-in reference doc for the consumed C3 surface); `Command` enum + usage-registry entries for `pilot pair|run|disconnect|status`; dispatch arms | none — same binary, same install path, same self-update channel |
| user-local state | `config_dir()/pilot-targets.json` (0600 metadata map) + `config_dir()/pilot-secrets.json` (0600 headless-fallback verifier map) + keyring entries `ox/pilot/{instance}/{connector_id}` | none — binary-owned files in the user's own config dir; not operator-provisioned infra |
| deps | `secrecy`/`zeroize` (SecretString), `base64` (fs_* payloads — matches the workspace's existing pin), a process-group-signal path (`rustix`/`nix`/`libc` or `kill` shellout — decided at codegen per the deps table in nfr-design/logical-components.md) | committed `Cargo.lock`; `--locked` installs; dependabot weekly cargo batch |
| server side | **zero** — pairing codes, target rows, C3 routes, audit are all U6's already-built surface; the client is outbound-HTTPS-only | none |

## Environment

No new `OXIDEAN_*` variables anywhere — client-side custody is
keyring + file fallback, never env; server-side enablement
(`OXIDEAN_OXPILOT_*`) is U4/U6's existing surface. `.env.example`
and `docs/CONFIGURATION.md`'s env table are untouched by this unit
(the doc surface that DOES change is the connector runbook — see
cicd-pipeline.md §Docs surfaces).

## Distribution surface (existing — nothing added)

The pilot verbs ride the `ox` release pipeline unchanged:

| channel | how U7 rides it |
|---------|-----------------|
| `ox-cli.yml` build matrix | path-filters `crates/oxidean-cli/**` — every PR touching `src/pilot/` triggers the 9-target matrix (linux gnu/musl ×3 arches, macOS ×2, Windows ×2) + FreeBSD VM leg + platform smokes (`scripts/ox-smoke.{sh,ps1}`) |
| release lane | `ox-v*` tag → same matrix builds release assets; `.sha256` digests + `SOURCE_COMMIT` verify chain unchanged |
| packages/installers | nfpm packages + `install.sh`/`install.ps1` + `cli_dist.rs` self-update — all binary-level, verb-agnostic |
| version registry | `oxidean-cli` already has a `versioning.toml` component; no new component registers |

## Operator provisioning

Zero. The user's side needs `ox` + their own `config_dir()`; the
pairing-code → verifier exchange IS the credential flow (single-use
600s-TTL code minted in the UI, exchanged at `ox pilot pair`). No
pre-issued tokens, no inbound firewall rule (outbound HTTPS only),
no per-user resources for an operator to create. The runbook IS the
doc artifact: mint code in UI → `ox pilot pair` → `ox pilot run
--root <dir>`.

## Resource shape (client process)

- One `ox pilot run` per target; RSS is bounded by the 64 KiB exec
  tail buffer + ≤~171 KiB fs payload + reqwest/serde footprint —
  no unbounded user-controlled growth surface (RunState bounds in
  nfr-design/scalability-design.md).
- Request budget: ≤4 heartbeats/min + ~1–2 claims/min + reports on
  demand — self-limiting at the 60s backoff ceiling on flap.
- No ports, no IPC surface, no daemon mode — a foreground process a
  supervisor (systemd/launchd/Task Scheduler) may wrap; the
  exit-code contract (0=cleanly-retired, 1=attention) is the
  supervisor interface.
