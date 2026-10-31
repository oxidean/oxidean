# Infrastructure Design Questions — connector-client

Unit: `connector-client` (U7, kind `service` — greenfield) —
`OxpilotConnector` client: `ox pilot pair|run|disconnect|status`,
`src/pilot/{mod,client,custody,exec,state}.rs`, wiremock C3 suite.
Infra concerns: what CI must cover, what the operator must NOT
provision, and which drift-gates the new verbs trip.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U7.
- [design] `construction/connector-client/{functional-design,
  nfr-requirements, nfr-design}/*` — the seams CI must cover.
- [contract] `contract-summary.md` (C3) — connector HTTP surface the
  client consumes.
- [team] `team.md` — Testing Posture (nextest archive → api-rust
  shards), Code Style, Deployment (ox-v* release matrix).
- [ci] `.github/workflows/{ci,ox-cli}.yml`,
  `scripts/check-cli-parity.sh`, `docs/CLI-PARITY.md`,
  `crates/oxidean-cli/Cargo.toml` (wiremock dev-dep).

## Q1. Does connector-client add deployable infrastructure?

- A. None — the connector runs on the USER's machine as `ox` verbs
  inside the existing `oxidean-cli` binary. No new service,
  container, port, or operator-provisioned resource server-side;
  `config_dir()` writes (`pilot-targets.json`, `pilot-secrets.json`)
  are user-local files the binary owns, not infrastructure.
- B. A connector-sidecar deployable — inverts the model; the
  connector IS the user's process reaching OUT (U6's
  outbound-only posture).
- C. A per-user download channel for a separate connector binary —
  `ox` already ships the 10-target release matrix; a second binary
  duplicates the whole release/verify path.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — zero new deployables; user-side ox verbs in the existing binary; config_dir files are binary-owned, not infra

## Q2. Which existing CI lanes must cover the new code — and do they?

- A. Two lanes, zero edits: (1) `ci.yml` `rust-build` compiles
  `cargo nextest archive --workspace --features server`, so
  `tests/pilot_connector.rs` (wiremock suite) + `src/pilot/*` unit
  tests replay in the `api-rust` shards automatically — the
  archive is workspace-wide, no `-E` filterset term needed
  (contrast U6's dialect_oxpilot.rs which DID need one);
  (2) `ox-cli.yml` path-filters `crates/oxidean-cli/**`, so every
  PR touching `src/pilot/` triggers the 9-target build matrix +
  FreeBSD VM + platform smokes — `cmd /C`/`taskkill /T` (Windows),
  `killpg`/`process_group` (unix), and `sh -c` paths all get
  compile+smoke coverage. `wiremock = "0.6.5"` is already a
  dev-dep of the crate.
- B. A dedicated `pilot` CI job — duplicates the workspace archive
  and the ox matrix for zero new signal.
- C. Stack-e2e coverage — U7's connector needs a live forge; the
  wiremock suite IS the contract coverage (Q7-A at nfr-design:
  "fixtures are NFR9's client half"); stack coverage of the
  full pair→run flow belongs to U8's story.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — nextest archive (workspace-wide, no filter edit) covers tests/ + unit tests; ox-cli.yml path filter covers the 9+1 target matrix + smokes; wiremock already a dev-dep

## Q3. Which drift-gates/docs surfaces does `ox pilot` trip?

- A. `docs/CLI-PARITY.md` — `check-cli-parity.sh` greps the matrix
  for `implemented` rows and asserts `ox <group> --help` parses;
  `pilot` group rows (pair/run/disconnect/status) must land in the
  same change or the check either misses them or fails. Plus the
  help-carrying surfaces: `docs/CONNECTOR.md` (or CONFIGURATION
  section) with the jail/cap/exit-code contract, and `--help`
  text — Q7-A at nfr-design. No new `OXIDEAN_*` env vars client-
  side (custody is keyring/files, not env), so `.env.example`
  untouched.
- B. CLI-PARITY is gh-vs-ox only — it gates whatever rows are
  marked `implemented`; new verbs without rows are invisible but
  the drift-gate intent (every claimed verb parses) argues for
  registering them anyway.
- C. A new OpenAPI surface — the client consumes C3 over plain
  HTTP; `src/pilot/openapi.yaml` is a checked-in REFERENCE doc for
  the consumed surface, not a generated/drift-gated artifact.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — CLI-PARITY.md rows for the pilot group in the same change + CONNECTOR/CONFIGURATION docs + help text; openapi.yaml is reference-only; no env vars

## Q4. Monitoring/observability infrastructure for the connector?

- A. None server-side to build — the connector's observability is
  stderr + exit codes + `--verbose` + `ox pilot status` (client
  surface, NFR6.x). Its server-visible liveness rides U6's
  `last_seen_at` + heartbeat window + audit rows — already built.
  No new dashboards, alerts, or log sinks in this unit.
- B. A per-target health endpoint — liveness is a server-side
  derived field (90s window); the client just keeps ticking.
- C. Connector-side metrics exporter — no metrics surface exists
  anywhere in `oxidean-cli`; inventing one for this unit breaks
  the established stderr+exit-code shape.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — client-side stderr/exit/status is the whole surface; server-side liveness is U6's built last_seen+heartbeat+audit

## Q5. Packaging/distribution changes for the pilot verbs?

- A. None — the verbs ride the existing `ox` binary: the 10-target
  `ox-cli.yml` matrix (9 + FreeBSD VM), `.sha256` digests,
  `SOURCE_COMMIT`, nfpm packages, install.sh smokes, `ox-v*` tag
  release lane, and `cli_dist.rs` self-update channel all pick up
  `src/pilot/` code automatically — it compiles into the same
  binary.
- B. Feature-gate pilot behind a cargo feature — the verbs are
  small, always-on user surface; a feature flag complicates the
  release matrix for zero footprint win.
- C. Version-gate the verbs on server oxpilot enablement — the
  client already learns `disabled` at pair time via the gate;
  compile-time gating can't see runtime state.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — rides the existing ox binary + 10-target matrix + checksums + release lane; no feature gate, no separate package

## Q6. New crate dependencies — supply-chain treatment?

- A. The R-09-dep-table gaps (`secrecy`/`zeroize` for SecretString,
  `base64`, a killpg path — `rustix`/`nix`/`libc` or `kill`
  shellout) are ordinary cargo deps: `Cargo.lock` is committed,
  CI installs with `--locked`, and dependabot's weekly cargo
  batch picks them up. Pin exact versions; `base64` matches the
  workspace's existing 0.23 pin (TS-8's "existing pattern"
  clause). No new install path, no vendoring.
- B. Vendor the killpg syscall inline via `std::process` escapes —
  `Command::process_group` creates but cannot signal the pgid;
  an unmanaged syscall shim duplicates what `rustix`/`libc`
  already maintain cross-platform.
- C. Pull a full async-process crate (`tokio::process` is already
  linked via tokio) — the dep question is only the GROUP SIGNAL,
  which tokio doesn't provide; no larger crate needed.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — ordinary pinned cargo deps in committed Cargo.lock; base64 matches workspace pin; dependabot batch covers freshness; no vendoring

## Q7. Anything the OPERATOR must provision for a user to pair?

- A. Nothing — the user's side needs only `ox` + their config_dir;
  the server side (pairing codes, target rows, capability
  negotiation, C3 routes) is U6's already-built surface. The
  connector doc (Q3-A) IS the provisioning story: mint code in
  UI → `ox pilot pair` → `ox pilot run --root`.
- B. A per-user connector token issued by the operator — the
  pairing-code → verifier exchange IS the token flow (single-use
  code, TTL 600s); no pre-provisioned secret exists by design.
- C. Firewall/ingress rules for connector traffic — the client
  only makes OUTBOUND HTTPS to the forge; nothing listens.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — zero operator provisioning; pairing-code exchange is the token flow; client is outbound-only; docs carry the runbook

## Consolidated Summary Confirmation

- No deployable infra: user-side `ox` verbs, config_dir files are
  binary-owned state (Q1).
- CI: workspace nextest archive covers tests automatically;
  `ox-cli.yml` path filter covers the 9+1 target matrix + smokes
  (Q2).
- Drift gates: CLI-PARITY.md rows + CONNECTOR/CONFIGURATION docs +
  help text in the same change; no env vars (Q3).
- Monitoring: stderr/exit/status only; server-side liveness is
  U6's built surface (Q4).
- Packaging: existing ox binary + matrix + checksums + release
  lane; no feature gate (Q5).
- Deps: pinned cargo deps + committed lock + dependabot; base64
  matches workspace pin (Q6).
- Operator provisioning: zero — pairing code is the token flow,
  outbound-only client, docs are the runbook (Q7).

[Answer]: Looks correct
