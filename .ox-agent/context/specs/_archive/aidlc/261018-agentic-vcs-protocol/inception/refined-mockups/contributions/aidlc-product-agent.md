**Collaborator:** aidlc-product-agent

## Contribution

1. [blocking] **Wrong CLI output variant cited — followed literally it breaks the exit-code contract.** `mockups.md`, `design-system-mapping.md`, and `interaction-spec.md` anchor on "`Out::Report` … `--json` prints the RPC payload verbatim, `{ok:false}` drives the exit code." In code, `Out::Report {text, json}` is the *local non-RPC* variant and always returns `ExitCode::SUCCESS` (`commands.rs:94-96`; `main.rs:38-50`). The described behavior is `Out::Rpc {envelope, pretty}` — envelope verbatim under `--json`, error → `rpc::error_exit_code` (`commands.rs:99-102`; `main.rs:52-91`). Implemented via `Out::Report`, every `ox vcs` failure would exit 0, voiding the state/exit-code map and the a11y invariant "non-zero exit on every typed failure". Fix the anchor to `Out::Rpc`.

2. [blocking] **`"repo":"myorg/backend"` combined-arg shape doesn't exist in wire or authz conventions.** Every repo-targeted RPC input uses separate `{owner, name}` (`repo_types.rs:800-815` `RepoTreeRequest`; `proc_classes.rs:22` "({owner,name}|repository_id input)"); the PAT repo-target resolver reads `owner` + `name`|`repo`-as-name or `repository_id` — `repo` means the *name only* (`bearer.rs:497-514`). MCP tool schemas build required `owner`/`name` props (`mcp.rs:590-600`), and `fg_repo_access_check` extracts exactly those fields, **failing open** to `Ok(())` when absent (`mcp.rs:1041-1052`). The `{"repo":"myorg/backend"}` shape shown in S3 and S4 would silently skip the tool-layer FG-grant check the spec claims (AC8.3.2). Render `{owner:"myorg", name:"backend"}` args, or record the combined convention as an explicit contract decision with the authz-extraction update it requires.

3. [suggestion] **Invented exit codes 3/4.** `mockups.md` shows `vcs.ref_conflict` → `exit code: 3`; `vcs.conflict` → `exit code: 4`. Existing `error_exit_code` maps only `auth.*`/`admin.forbidden` → 3, everything else → 1 (`rpc.rs:52-65`) — under it both would exit 1, and `vcs.unauthenticated`/`vcs.forbidden` would *also* exit 1, not 3. Either align to the table's "non-zero" wording or record a deliberate `error_exit_code` extension — and if extended, map `vcs.unauthenticated`/`vcs.forbidden` beside their `auth.*` siblings.

4. [suggestion] **`--change` grammar cannot express AC2.6.1 (Must).** The shown `path:kind:[from:]@sha256:…` has no slot for mode/executable/symlink (`100755`, `120000`), though the MCP args carry `"mode":"100644"`. As drawn, the CLI can't commit an executable or symlink. Add a mode field or document the default+override before contract freeze.

5. [suggestion] **`vcs.status` shape is invented.** S2.1 renders `changesets`, `refs{}`, `objects:{count,bytes}`; the doc assigns the `git_oid ↔ changeset_id` mapping query to `vcs.status`. Requirements fix only "live + empty history" (AC1.1.2) and "mapping is queryable" (AC7.1.1) — no fields, no named proc. Mark provisional: the oid-map lookup wants its own arg/procedure, not a status field.

6. [suggestion] **Ava's day-one flow unshown: bootstrap commit + inline transport.** S2.2 only shows `--parent` + `@sha256:` refs; the empty-parent root commit (AC2.1.4) and the inline-contents blob transport (AC2.1.2 — "two distinct request fields") appear nowhere. One example line each.

7. [suggestion] **Ravi's trust loop shows only the refusal.** S2.6 renders export-denied but never the successful `ox vcs export … --to main` → single branch move (AC7.2.1). The landing beat is the payoff; add it.

8. [suggestion] **Coverage notes overclaim.** Claims "US4.x reads (log/diff/tree/blame/history + pagination cursor)" — only `log`, `diff`, `checkout` are rendered. `blame` per-line shape, `tree` listing, `path_history`, `file_get`/`show`/`cat`, `merge-base`, `ref list` outputs never appear. Render schematically or narrow the claim.

9. [suggestion] **No empty states on machine surfaces.** Fresh-repo `vcs log` → empty entries, `ref list` → `[]` (AC5.2.1), `status` → 0 changesets. The state tables carry no empty row.

10. [suggestion] **Object-route error HTTP status unstated.** S4 shows RPC domain errors as `→ 200 {ok:false}` but the object GET error returns a bare envelope with no status. On a mixed-media route (200 octet-stream success vs JSON error) Kit needs 404-with-envelope vs 200-with-envelope pinned.

11. [nit] **stderr format diverges from convention.** Mockups print `error: <code> — <message>`; existing CLI emits `ox: <code>: <message>` (`main.rs:76,93`). Pick one.

12. [nit] **`--json` error destination unstated.** `Out::Rpc` prints error envelopes to *stderr* under `--json`; the state table doesn't say where `--json` failures print.

13. [nit] **Cheap missing beats:** `vcs.rebase` conflict example (same `{path,kind}` shape), a US2.3 "second commit on same parent → distinct id" line, a US6.2 "0 fetched (all cached)" checkout summary, US6.3 attribute note.

14. [nit] **`--all` flag orphaned.** Q8 answer promises `--all` with a "may page many times" note; C2's Inputs table omits it.

15. [nit] **Checkout's client-side typed failure (AC6.1.3 — non-empty/conflicting target) has no exit-map row.** It's a local error, not `vcs.*`; show its `error:`/exit shape.

16. [nit] **MCP has `vcs_ref_list` but no `vcs_ref_update`** (faithful to FR13.1) — an MCP-only Ava can't move her per-task head without `ox api`/raw RPC. Deliberate, but one sentence prevents it reading as an omission; same for checkout's absence.

## Positions

Endorse unchanged:

- **Q1=B surface inventory**: one Octane control + machine surfaces, activity feed as the only human-visibility surface. The minimum that honors O5 while giving Sana a non-CLI path.
- **Typed-code-everywhere posture, including the human surface**: S1's `"Couldn't update: vcs.forbidden"` inline text — the AC8.1.4 taxonomy reaching the UI instead of being flattened.
- **CAS ref UX honesty**: `--expect/--to`, stale-expectation error echoing the actual value, expected-absent/null-target sentinels — matches AC5.1.x exactly.
- **Export ≡ push verdict equivalence**: "identical verdict text to a rejected `git push`" is the trust loop's keystone; `--force` "never protection verdicts" lands AC7.2.2 precisely.
- **isError domain-denial discipline + typed `data` payloads** — agents never scrape message strings.
- **Idempotent resubmit line** and **want-based fetch made visible** — the two protocol properties Ava/Kit care most about, shown not asserted.
- **Activity-feed granularity decision**: one row per op, no per-object rows, actor/kind-filterable.
- **Terminal accessibility analogues**: no color-only semantics, stderr discipline, `--json` parity.
- **Pagination contract** `{limit,cursor}` → `{entries, cursor?}` with `vcs.diff` boundedness explicitly parked to R-14.
- **Toggle copy**: "Turning this off stops new writes; existing history stays readable" is AC1.2.1/FR1.4 in one sentence.
