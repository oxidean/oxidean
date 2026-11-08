## Review

**Stage:** code-generation
**Unit:** u8-vcs-cli
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

None outstanding. The floor-18 iteration-1 review (attempt `644ce928f29f5d02`)
found one MAJOR — `vcs.ref_update` wire field misnamed `ref` instead of
`ref_name`, which would have failed every `ox vcs ref set` server-side —
plus two NITs (`opt_limit` u64→i64 wrap; merge `fast_forward` pretty — the
second a false alarm, already rendered). All resolved and re-verified in
the bound bytes:

- `cli.rs` sends `ref_name` and pretty-reads `data["ref_name"]`; the e2e
  fixture asserts the corrected wire body.
- `opt_limit` bounds `--limit` via `i64::try_from` → usage error.

### Verified clean

- Request field names for all 17 procedures match the typed wire DTOs
  (`ref_name`, `changeset_get.id`, `log.from` required `String`,
  `diff.from/to`, `tree.changeset_id`, `blame.at`,
  `rebase.new_parent_id`, `git_export.head/branch/force`).
- `checkout` pins `<cs>` required per plan row 56 — no ref name reaches
  `vcs.tree`'s strict `ChangesetId` field.
- `PathFilter::matches` empty-set → full-checkout; non-empty-target
  refusal; marker-owned overwrite; atomic temp-write-rename; modes/symlink.
- `CliFault::Server(Box<RpcResponse>)` preserves `{ok:false}` envelopes
  verbatim through RPC and `Envelope` object-route channels;
  `vcs.unauthenticated`/`vcs.forbidden` → exit 3.
- Pre-existing `-R` + positional-args bug in `repo_and_arg` (positionals
  shadowed the `-R` repo on main) repaired via `flag_repo`;
  `repo_and_two_args` shares the fix.
- Feature boundary: client-only build pulls zero server deps.

### Summary

42 u8 tests (29 client + 9 parser + 4 e2e) plus full workspace nextest
green; clippy clean on touched code. READY.
