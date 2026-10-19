## Review

**Stage:** code-generation
**Unit:** u8-vcs-cli
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

### Scope

`crates/oxidean-vcs` `client` feature (`transport`, `object_cache`,
`working_copy`, `cli`, `mod`, lib/Cargo wiring) plus the `oxidean-cli` argv
glue (`args.rs` parser + completion, `commands.rs` transport/dispatch/gating,
`http.rs` object routes, `rpc.rs` exit codes, `main.rs` `Out::Bytes`, tests).

### Findings

**MAJOR — `vcs.ref_update` wire field misnamed (`ref` vs `ref_name`).**
The RefSet arm sends `input["ref"] = name` and pretty-reads `data["ref"]`,
but `VcsRefUpdateRequest` requires `ref_name` (vcs_types.rs:863) and
`VcsRefUpdateResponse` echoes `ref_name` (:875). The host handler
deserializes the typed request (vcs.rs:233), so every `ox vcs ref set` call
fails `rpc.bad_input` server-side — the entire CAS write surface is dead on
the wire, and the pretty line never sees the echoed name. Production sites:
`cli.rs:641`, `:660`, `:661`; fixtures asserting the wrong field at
`cli.rs:1365`, `:1389` and `commands_e2e.rs:828/832`. Fix: send/read
`ref_name` end-to-end and update the fixtures.

**NIT — `opt_limit` u64→i64 narrowing.** `args.rs` casts `opt_u64` with
`as i64`; inputs above `i64::MAX` wrap negative instead of failing at the
CLI. The server bounds-checks `limit`, so the blast radius is a confusing
`rpc.bad_input` rather than silent corruption — still worth a saturating
check or a range error at parse time.

**NIT — merge pretty omits `fast_forward`.**
`VcsMergeResponse.fast_forward: Option<bool>` is part of the contract
response but `merge_pretty` renders only `changeset_id`/conflicts, so a
fast-forwarded merge is indistinguishable from a merge-commit result in the
pretty line. Optional polish; the JSON envelope is intact for `--json`.

### Verified clean

- Request field names for the other 15 procedures match the typed wire DTOs
  exactly (`changeset_get.id`, `log.from` required `String`, `diff.from/to`,
  `tree.changeset_id`, `blame.at`, `rebase.new_parent_id`,
  `git_export.head/branch/force`).
- `checkout` pins `<cs>` as a required positional per plan row 56 — no
  illegal ref name reaches `vcs.tree`'s strict `ChangesetId` field.
- `PathFilter::matches` empty-set → full-checkout semantics correct.
- `CliFault::Server(Box<RpcResponse>)` preserves `{ok:false}` envelopes
  verbatim through both the RPC and `Envelope` object-route channels;
  `vcs.unauthenticated`/`vcs.forbidden` reach `error_exit_code` → 3.
- Pre-existing `-R` + positional-args bug in `repo_and_arg` (positionals
  shadowed the `-R` repo on main) correctly repaired via `flag_repo`;
  `repo_and_two_args` shares the fix.
- Feature boundary: client-only build pulls zero server deps.

### Required before READY

1. Rename `ref` → `ref_name` in the RefSet request insert and both
   pretty-read sites; update the three fixtures asserting `"ref"`.
2. Re-run the `oxidean-vcs` client tests and the `oxidean-cli` e2e suite.
