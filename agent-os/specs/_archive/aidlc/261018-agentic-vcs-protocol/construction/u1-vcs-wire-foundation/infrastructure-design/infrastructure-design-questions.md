# Infrastructure Design — Questions (u1-vcs-wire-foundation)

Library unit — no deployment surface, no runtime resources. The produce set is cicd-pipeline.md + traceability.json: which existing CI lanes cover this unit's changes and whether any new lane is warranted.

**Q1 — Does U1 need any new CI lane?**

[Answer]: No. Its changes are fully covered by existing lanes: `test` (nextest — coverage tests + serde round-trips), `rpc-sync-check` (client parity), `version-check` (component↔workspace completeness), `dead-code-check` (machete over the dep-free skeleton + clippy -D dead_code), `web-lint`/`web-format-check` (activity-row null guards + generated client), `commit-lint` (the `wire`-scoped breaking commit for the RepoActivityItem widening). The only new *check* is the WIRE_VERSION sync assertion, which lands as a Rust test — inside the existing test lane, not a new lane.

**Q2 — Any build-order or caching concern from minting crates/oxidean-vcs?**

[Answer]: None — the skeleton is zero-dependency; CI compiles workspace crates from fresh checkout (cargo-chef-cooked dep layers unchanged). The crate enters the members list atomically with its versioning.toml registration in the same commit — `version-check` requires member↔component bidirectionality.
