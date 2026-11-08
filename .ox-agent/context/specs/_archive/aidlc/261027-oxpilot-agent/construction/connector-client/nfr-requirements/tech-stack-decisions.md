# Tech-Stack Decisions — connector-client (U7)

Consumes: functional-design questions Q1/Q11/Q12 answers,
`codekb/oxidean-app/technology-stack.md`, existing
`crates/oxidean-cli/src/{args,commands,config,keystore,device,http}.rs`
seams, `tests/rpc_http.rs` wiremock conventions.

## Decisions

| id | decision | rationale |
|----|----------|-----------|
| TS-1 | Implementation lives in `crates/oxidean-cli` as a `src/pilot/` module dir (mod + client + exec + state) with `args.rs`/`commands.rs` registration — NOT a new crate | The connector IS an `ox` verb set; it rides the CLI's existing instance resolution, keystore, config_dir, and http seams for free (Q1-A). A separate binary would re-implement all four seams |
| TS-2 | HTTP via the crate's existing client conventions (`device.rs` precedent: reqwest + 30s timeouts + flat error envelopes) — no new HTTP stack | Device flow already speaks Bearer + JSON + timeout + Retry-After; the connector adds only the long-poll claim variant |
| TS-3 | Credential custody via `keystore::{set,get,delete}(instance, token)` — composite instance key `pilot/<instance>/<connector_id>` — plus `pilot-secrets.json` 0600 as the defined headless fallback | The seam exists and is instance-scoped; the fallback is the only piece genuinely new (BR2.4). No new secrets crate (Q2-A rejected) |
| TS-4 | Identity metadata in `pilot-targets.json` (0600) under `config_dir()` — a machine-local map keyed by `target_id`; never `config.json` | `config.json` is user-edited; machine-minted rows invite corruption and its schema doesn't model target/caps (Q3-C rejected) |
| TS-5 | Process control: `std::process` + `killpg`/`taskkill /T` for process-group kills; `sh -c`/`cmd /C` platform shells | stdlib covers the serial executor — no async runtime needed beyond what the crate already links |
| TS-6 | Tests: `wiremock` fixture suites mirroring `tests/rpc_http.rs` + no-network executor unit tests | wiremock is already the crate's outbound-HTTP fixture tool (NFR9's client half); a live-forge dependency is rejected (Q7-A) |
| TS-7 | `openapi.yaml` for the client-consumed C3 surface checked in under the pilot module docs — enumerates the full flat `{"error":"<code>"}` vocabulary (all 10 codes) | The client IS the contract consumer; pinning the vocabulary in-repo keeps codegen honest (U6 C3 delta) |
| TS-8 | No new dependencies beyond what `oxidean-cli` already links unless codegen finds a genuine gap (e.g. base64 — prefer the workspace's existing base64 usage pattern) | Dependency discipline per AGENTS.md — extend existing patterns, don't add frameworks |

## Rejected alternatives

- Standalone `oxpilot-connector` binary — duplicates instance
  resolution, keystore, config_dir, and HTTP seams (TS-1).
- Verifier in `pilot-targets.json` — collapses the custody split;
  rejected at functional design (TS-3/TS-4).
- Generic secret-pattern output scanning — false-positive shreds and
  unpredictable cost vs the enumerated-value scrub (Q3-C).
- Live-forge integration tests — unavailable in connector CI lanes;
  fixtures are the NFR9 answer (TS-6).
