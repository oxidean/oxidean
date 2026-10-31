# Infrastructure Specification — oxpilot-tools (U5)

Consumes: `nfr-design/*`, `nfr-requirements/*`,
`functional-design/{rules,entities,functional-spec}.md`,
`inception/domain-design/components.md` (component inventory),
`contract-summary.md` (C1/C6/C7), `team.md` (Way of Working).

## Deployable surface

U5 adds **no deployable**. The catalog is in-process library code
inside `oxidean-api-oxpilot` (`tools/{mod,registry,validate,read,
propose,dispatch,comment,procs}.rs`). What ships:

| surface | change | infra impact |
|---------|--------|--------------|
| `oxidean-api-oxpilot` | eight `tools/*.rs` modules — closed `&'static [CatalogEntry]` registry (23 rows), closed-vocabulary validator, handlers | none — same binary, same image |
| `oxidean-api-core` | `ProposalWrite.ops_json` field on the shipped write type | none — library type |
| `oxpilot.diff_{list,get,apply}` procs | three USER-ONLY verbs on the closed PROCEDURES set + dispatch arms | rpc-gen drift check only |
| `OxpilotTools` install | one `Arc::new(OxpilotTools::new(state.clone()))` onto `state.oxpilot.slot.tools` at shell assembly (main.rs, beside `install_sessions`/`install_gateway`) | none — in-process seam |
| migration `0089_oxpilot_proposals_ops.sql` | additive `ops_json` column on existing `oxpilot_proposals` ×3 dialects; applied via the existing boot runner | append-only migration, same as every table |
| deps | none — serde_json/tokio/tracing already in tree | none |

## Environment

No new variables. `require_enabled` reads the existing
`OXIDEAN_OXPILOT_*` flag/provider chain (U1/U4's surface);
`require_verified` reads the caller's session; tools hold no
provider credentials (provider auth lives in U4's adapters,
connector custody in U6/U7). `.env.example` and
`docs/CONFIGURATION.md` untouched by this unit.

## Restart invariants

- `OxpilotTools` is stateless by construction — an
  `Arc<dyn ToolExecutor>` on the runtime slot serving every
  session; boot state is the static registry only (NFR-S1/S2).
- A failed mid-chain `diff_apply` leaves a deletable
  `oxpilot/proposal-<id8>` branch and a `pending` proposal row —
  never a mutation on the target branch (NFR4.11/4.12); both are
  operator-visible leftovers, not process state.
- The `ops_json` column is additive and nullable: pre-existing
  rows read as none; a code rollback leaves a harmless unused
  column — no down-migration (append-only policy).
- Single-replica caveat inherited unchanged from U3's posture:
  the slot install lives inside the one `api` process (NFR-S1).

## Resource shape

- Per-invoke work is bounded: catalog lookup O(23) fixed-order,
  schema walk O(keywords × nodes) at depth ≤ 16 vs authored ≤ 3,
  serialized action args ≤ `ACTION_ARGS_MAX` 2 KiB
  (bound-and-send the same string).
- `diff_apply` ≤ ~53 sequential `rpc_dispatch` calls — bounded
  by `ops_json` (≤50 ops + branchCreate + pull.create), each
  carrying the caller's authz.
- No new connection pools, threads, channels, caches, or
  background tasks — `emit_card` borrows the existing turn map,
  `dispatch_action` borrows the gateway's action queue,
  tool-call persistence is U3's row, `ArtifactScope` is rebuilt
  per call and dropped (NFR-S6).
