## Review

**Verdict:** NOT-READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Date:** 2026-10-14T12:00:00Z
**Iteration:** 1

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Major | contract-summary.md §C6 | Wire-version transition semantics are unspecified at the one boundary that is a hard gate. The server enforces `check_version_header` on every RPC call (`rpc.rs:105–117`) — a hard `rpc.version_mismatch` rejection. Every shipped `ox` sends `Oxidean-RPC-Version: 1`; under C6 the server caret-compares against `WIRE_VERSION` `0.1.0`, so `"1"` either fails parse or is incompatible — every installed CLI hard-breaks. Reverse direction equally unaddressed (new CLI → old server's exact-match `1`). C6 also never states the fate of `protocol_version` in `system.manifest` or client behaviour when `wire_version` is absent (`Manifest.protocol_version` serde-defaults to 0). For a self-hostable product whose premise is CLI/server drift, this transition rule is the contract's core content. | State in C6: (a) the accepted-value transition rule for the header; (b) whether `protocol_version` is removed or co-emitted and what clients do on absence; (c) client behaviour when `wire_version` is absent. | New |
| R-02 | Major | contract-summary.md §C5; `cli_dist.rs`; `ox-cli.yml:426–470` | `latest.json` under-specified on three axes. (i) Field mapping drifts from the live `/cli/latest` response (`targets: [{target, sha256, bytes}]` + optional `dist_commit`); `url`/`triple`/`bytes`/`dist_commit` semantics unpinned — whether the endpoint serves manifest targets, translates fields, or keeps directory-scanning and takes only `version` produces three different endpoints. (ii) Writer lifecycle: rolling `ox-dist` is refreshed on every main push while C5 names only the `ox-v*` tag flow as writer — which job emits it, on which triggers, with what version source, is unpinned. (iii) `channel: stable\|prerelease` has no FR basis; drop-in merge with `OXIDEAN_CLI_DIST_DIR` binaries and the absent-file value are unstated. | Pin file↔response field mapping, writer job/triggers/version source, `channel` provenance or drop it, absent-file value, drop-in merge semantics. | New |
| R-03 | Minor | contract-summary.md §C1 vs components.md RegistryEntry | C1 naming drifts from the entity model: `kind: cargo\|bun\|constant` vs `crate\|bun\|contract`; `tag` vs `tagPrefix`. `manifest`/`constant` both optional with no kind-correspondence rule. | Pick canonical names (or declare C1 authoritative); state manifest-xor-constant-by-kind validation. | New |
| R-04 | Minor | contract-summary.md §C2 | `bump_signal` listed as parser output while its comment says the 0.x table is applied by caller context — the parser has no version input and cannot emit the final signal. Unstated whether `commit-check` treats `scope_status: unmapped` as warning or failure. | Relabel/remove `bump_signal`; state unmapped-scope severity for the lint surface. | New |
| R-05 | Minor | contract-summary.md §C4 | Trigger matrix omits the FR8.2 `workflow_dispatch`/dedicated-credential alternative lane and prerelease-tag semantics (`ox-v*.*.*` globs match `ox-v0.2.0-alpha.1` — prereleases would enter the stable lane and potentially write `latest.json`). | Cover the dispatch alternative; state whether prerelease tags fire release lanes and how they interact with C5. | New |

### Summary

Structurally complete — all six boundaries specified, ownership rules coherent, open-questions table present — but NOT-READY on two Majors: C6 never adjudicates mixed-version interop at the hard `Oxidean-RPC-Version` gate, and C5 leaves `latest.json` field mapping, writer lifecycle, and channel/url semantics undefined. Three minors cover C1 naming drift, C2's misplaced `bump_signal` + unmapped-scope severity, and C4's missing prerelease/`workflow_dispatch` trigger semantics.
