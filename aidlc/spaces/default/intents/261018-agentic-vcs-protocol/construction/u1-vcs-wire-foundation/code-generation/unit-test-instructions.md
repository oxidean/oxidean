# Unit Test Instructions — u1-vcs-wire-foundation

## Run

```bash
cargo nextest run -p oxidean-core -p oxidean-vcs     # wire DTOs, Nullable trichotomy, error codes, id newtypes
cargo nextest run -p oxidean-api                   # classification coverage, dispatch-arm stubs, rpc_gen tests
cargo nextest run -p oxidean-api --bin rpc-gen     # WIRE_VERSION sync + generated-client coverage
make version-check                                 # skeleton registration
make rpc-sync-check                                # client parity (green once generated index.ts is committed)
make web-lint && make web-format-check             # activity-row null guards
```

## What the tests prove

- `nullable_trichotomy` + friends: absent → `missing_field`, `null` → `Null`, value → `Value` (the serde `deserialize_option` trap is covered — plain `Option::deserialize` delegation silently maps absent→Null; the impl buffers through `serde_json::Value` instead).
- `vcs_classification` (proc_classes): the 19-name mapping incl. both route aliases; unlisted `vcs.*` stays SessionOnly.
- `vcs_procedures_registered_and_classified` + `vcs_dispatch_arms_return_disabled` (rpc.rs): all 17 in PROCEDURES, all classified, all stub arms emit `vcs.disabled`.
- `template_wire_version_matches_core` + `generated_client_covers_vcs_procedures` (rpc_gen.rs): the second WIRE_VERSION literal can't drift; all 17 names land in `index.ts`.
- `repo_activity_item_nullable_fields_emit_explicit_null` (repo_types.rs): widened fields serialize `field: null`, never absent.

## Coverage floor met

Standard strategy: unit tests per component + structural boundary assertions. No integration stubs needed — u1 has no runtime boundary (stubs return `vcs.disabled`; the real paths arrive with u7).
