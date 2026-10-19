## Review

**Verdict:** READY
**Reviewer:** aidlc-architecture-reviewer-agent
**Date:** 2026-10-18T06:45:00Z
**Iteration:** 1

### Findings

| ID | Severity | Location | Finding | Required action | Status |
|---|---|---|---|---|---|
| R-01 | Minor | entities.md:95 + memory.md | Stale vs LC3's split — entities.md still assigns the widening to u2 jointly; LC3 assigns DTO+wrap+web-guards to u1, DB side to u2. | Amend entities.md:95 to the same split; append correction to memory.md. | Unresolved |
| R-02 | Minor | LC3 | Second `row_to_item` edit unpinned — when u2 widens `RepoActivityRow` to Option, `Some(row.x)` fails to compile; u2 must edit oxidean-api's activity.rs (crate-boundary exception). | Pin in LC3: u2 removes the `Some()` wrap. | Unresolved |
| R-03 | Minor | LC5/LC6 | Dispatch-arm shape trap — the parity scanner only counts lines starting `"` whose text after the closing quote begins `=>`; a collapsed alternation arm like "vcs.a" / "vcs.b" combined into one `=>` line is invisible → every vcs.* entry fails. | Pin 17 separate single-name `"vcs.x" =>` arms. | Unresolved |
| R-04 | Minor | LC5 | u8 exit-code note omits the pinned decision — unit-of-work.md:66 already pins `vcs.unauthenticated`/`vcs.forbidden` → exit 3; `vcs.disabled` stays 1. | Quote the u8 pin. | Unresolved |
| R-05 | Minor | LC4 | Edge direction incomplete — `depends_on=[]` is correct only while `[dependencies]` is empty; the crate's own edges update when u3/u4/u8 add deps. | Add the clause. | Unresolved |
| R-06 | Minor | LC1 | Misdescribes `RefName` — entities.md:38 pins a plain `pub ref_name: String` + `#[serde(rename="ref")]`, not a validated newtype; `ObjectId` is an LC1 addition absent from the 13 entities. | Correct the description; note the ObjectId delta. | Unresolved |
| R-07 | Minor | LC3 | Serde attrs on the five widened fields unpinned — they must render explicit `field: null`, i.e. plain `Option` WITHOUT `skip_serializing_if` (the Q2 convention would emit absent). | Pin "no skip_serializing_if". | Unresolved |
| R-08 | Minor | LC-dep view | Omits LC5→LC6 edge; "LC1 used-by LC5 (rpc_gen template)" is a mirror edge (the template is a raw string literal, no type import); block should be ```mermaid. | Add edge, fix wording, mermaid fence. | Unresolved |
| R-09 | Note | SD1 + questions | "~240 lines" — proc_classes.rs is 639 lines; the match body is ~240. | Scope the count. | Unresolved |
| R-10 | Note | LC1/SD4 | `Nullable` enum must be `pub` (pub field of re-exported DTO — E0446); `#[schemars(with="Option<ChangesetId>")]` on the field is a lighter alternative to a manual impl. | Pin pub; note the schemars alternative. | Unresolved |
| R-11 | Note | SD2 | Pre-bootstrap, `vcs.*` RPC calls return `auth.setup_required` while manifest (bootstrap-allowlisted) already advertises the 17 — consistent with the pinned contract. | None | Resolved |
| R-12 | Note | LC3 | repo-activity-row.tsrx gets u1's null-guards now and u10's vcs-kind rendering later — acknowledge the two-touch. | One clause. | Unresolved |

### Summary

READY — no blockers. Verified: manifest parity tests genuinely require the stub names (procedures_list_matches_dispatch + manifest_shape); zero-dep skeleton legal (explicit member paths, literal-version requirement, machete/clippy over all members); JsonSchema is universally derived (manual impl would be the codebase's first — noted); dependency view acyclic; traceability SR1–SR5 coverage clean; FG-token `auth.pat_scope` vs `rpc.bad_input` layering precise.
