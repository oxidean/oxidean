<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: 2026-05-29T10:14:32Z — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
2026-10-18T10:00:00Z — (U1) treated the unit's "behavior" as three structural flows (request pipeline, registration completeness, regen cycle) rather than domain workflows — a wire-foundation unit has no state machines
2026-10-18T10:00:00Z — (U1, dev pass) the contract's "admin" tier resolves to PatProc::RepoAdminWrite — the repo-flag-toggle variant, no new variant needed
2026-10-18T10:00:00Z — (U1, dev pass) denial codes layer by surface: transport denials stay auth.*/rpc.unknown_procedure/repo.not_found; vcs.unauthenticated/vcs.forbidden are domain-layer and /api/vcs/*-route codes only

## Deviations
<!-- example: 2026-05-29T10:14:32Z — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->
2026-10-18T10:00:00Z — (U1) conflict.kind opened to string on the wire (U5's binary/mode/type/rename taxonomy) instead of a closed enum — a new kind is then additive under BR1.6
2026-10-18T10:00:00Z — (U1) per-DTO limit/cursor pagination fields chosen over a shared PageArgs type — matches how existing list DTOs are shaped; cursor model is a deliberate divergence from legacy offset+limit

## Tradeoffs
<!-- example: 2026-05-29T10:14:32Z — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
2026-10-18T10:00:00Z — (U1, dev pass) ref_update CAS fields encoded as required-nullable Option<ChangesetId> (absent → missing_field decode error) instead of a Nullable<T> newtype — serde required-but-nullable gives all three states with zero new machinery
2026-10-18T11:00:00Z — (U1, review R-01 correction) the Option pin above was WRONG — serde implicitly defaults Option<T> on absent keys, so absent ≡ null and no missing_field fires. Final encoding: `Nullable<ChangesetId>` newtype (enum { Null, Value(T) }, custom Deserialize); non-Option field → absent raises missing_field, null → Null, value → Value. The newtype IS the minimal correct mechanism.
2026-10-18T10:00:00Z — (U1, dev pass) RepoActivityItem's four push fields widen to Option (C4 nullability) — a wire-visible type change on repo.activity.list owned by u2's façade work, sequenced so schema+DTO change together

## Open questions
<!-- example: 2026-05-29T10:14:32Z — confirm the retention window with compliance before the next stage hardens the schema -->
2026-10-18T10:00:00Z — (U1, parked for u7) rpc_status's HTTP-mapping whitelist doesn't know vcs.* codes — they serialize HTTP 400 inside {ok:false} unless extended; acceptable since the typed code is authoritative, but u7's surface work should decide whether to extend
2026-10-18T10:00:00Z — (U1, parked for u7) /api/rpc Bearer accepts PAT + job tokens only; OAT-on-RPC is a host-level bearer.rs extension if we want it — contract's /api/vcs/* OAT path is unaffected

## Mob convergence record (1 support pass, integrated before review)
- Developer pass: 4 blockers all verified against code and folded — RepoAdminWrite tier pin, proc_classes route-alias exemption named in BR1.2, required-nullable CAS encoding in BR1.5, honest generated-client coverage check in BR1.8 (rpc-sync-check is template-parity only). ~15 suggestions folded: serde per-variant rename for dotted codes, ref_name+rename="ref" pattern, versioning.toml concrete entry, Vcs{Op}Request/Response naming + vcs_types.rs module, WIRE_VERSION dual-site pin, MCP grant fail-open caveat.
2026-10-18T06:50:00Z — (U1, nfr-design pass) correcting the earlier note: the RepoActivityItem widening does NOT land code+schema together — u1 owns the DTO widening + row_to_item Some() wrap + web-reader guards (nullable client types would break web-lint otherwise); u2 owns the insert signature + column nullability + removes the Some() wrap
