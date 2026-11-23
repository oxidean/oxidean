# WI-0001 implementation plan — COL-05 org teams

Spec: `artifacts/specification.md`. ADR:
`context/decisions/adr-2026-11-22-org-teams.md`. Linear stage chain; each
stage ends in a compile/test gate and (later) a conventional commit.

## Stage 1 — Schema + DB layer (oxidean-db)

- `migrations/{postgres,mysql,sqlite}/0105_org_teams.sql` — three tables +
  indexes per spec §3 (dialect-specific timestamp/DDL idiom copied from
  `0010_orgs_acl.sql` / `0104_saved_replies.sql` in the same dir).
- `crates/oxidean-db/src/org_teams.rs` — `OrgTeamRow`, `OrgTeamMemberRow`,
  `OrgTeamRepoRow` + list/join rows; fns per spec §7 ×3 dialects.
- `crates/oxidean-db/src/lib.rs` — `pub mod org_teams;`, row re-exports,
  `Database` façade wrappers.
- `crates/oxidean-db/src/repo_access.rs` — team-grant EXISTS (joining
  `organization_team_members` + `organization_members`) in
  `readable_repo_ids` + `readers_of_repo` ×3 dialects; doc-comment updated.
- Gate: `cargo test -p oxidean-db` green (migration parity test + any
  dialect tests), `cargo check -p oxidean-db`.
- Commit: `feat(db): add organization teams schema and store` — or fold
  with stage 2/3 into one backend commit; decide at commit time (squash
  merge makes intra-PR granularity cosmetic).

## Stage 2 — Core types + ACL (oxidean-core, oxidean-api-core)

- `org_types.rs`: `TeamRole`, `OrgTeamPublic`, `OrgTeamMemberPublic`,
  `OrgTeamRepoPublic`, `RepoTeamPublic` + all request/response types per
  spec §4.
- `repo/acl.rs`: `coalesce` gains `team: Option<Capability>` param;
  `effective_capability` queries the new team source for org-owned repos
  (suppressed by `org_locked`); coalesce unit matrix extended.
- Callers of `coalesce`/`effective_capability` — grep to update signature
  call sites.
- Gate: `cargo test -p oxidean-api-core` green; `cargo check -p
  oxidean-api-core oxidean-core`.

## Stage 3 — RPC handlers (oxidean-api-org, oxidean-api-repo)

- `crates/oxidean-api-org/src/teams.rs` — all `org.teams.*` handlers per
  spec §5 table (gates, errors, audit, sweeps); `members.rs` remove/
  update_role gain team-row cleanup (`delete_team_memberships_for_org_
  member`) before the existing sweep.
- `dispatch.rs` + `lib.rs` PROCEDURES updated (org crate).
- `crates/oxidean-api-repo/src/teams.rs` — `repo.teams.*` handlers
  (`resolve_repo_for_admin` gate, org-owned checks, upsert/remove, audit,
  sweeps); `dispatch.rs`/`lib.rs` updated.
- Tests: `crates/oxidean-api/tests/org_teams.rs` — full integration suite
  (AC 1–10) using the `tests/support` harness; repo-side cases may live
  here too.
- Gate: `cargo test -p oxidean-api --test org_teams` + existing
  `org_create_members`/`repo_collaborators_acl` still green; `cargo check
  -p oxidean-api`.

## Stage 4 — TS client (rpc_gen + regen)

- `crates/oxidean-api/src/bin/rpc_gen.rs` — TS types + `client.org.teams`
  and `client.repo.teams` call trees per spec §4/§5 shapes.
- `make rpc-gen` → regenerate `packages/api-client/src/index.ts`.
- Gate: `make rpc-sync-check` clean; `cargo check -p oxidean-api --bin
  rpc-gen` (template is Rust-embedded — verify compile).

## Stage 5 — Web UI (apps/web)

- `org-settings-nav.tsrx` + `"teams"` variant in `OrgSettingsNavActive`.
- `pages/[owner]/settings/teams.astro`, `pages/[owner]/settings/teams/
  [team].astro` shims (getStaticPaths per siblings).
- `routes/$owner.settings.teams.tsrx` — list + create dialog.
- `routes/$owner.settings.teams.$team.tsrx` — detail: profile/rename/
  delete + members tab (MemberLookup add, role select, remove) + repos tab
  (grant add/update/remove).
- `components/repo/teams-panel.tsrx` + mount in
  `routes/$owner.$repo.settings.tsrx` (org repos only).
- Query keys per spec §8; `useQuery`/`useMutation` (or queryClient
  invalidate) per members-page precedent; `useForm` for create.
- Integration test(s): `$owner.settings.teams.integration.test.ts` +
  `$owner.settings.teams.$team` coverage — happy-dom per
  `orgs.new.integration.test.ts` pattern.
- Gate: `make web-lint` + `make web-format-check` green;
  `bun run --filter @oxidean/web test` scoped files pass.

## Stage 6 — Bookkeeping + ship

- Roadmap COL-05 `[~]` → `[x]` with shipped summary.
- Final gate sweep: `cargo fmt --check` touched crates, clippy scoped,
  `make rpc-sync-check`, `make web-lint`, `make web-format-check`, scoped
  tests re-run; then `git fetch origin && git merge origin/main`, push,
  PR with test-plan checklist, `@coderabbitai review` comment.

## Risks / assumptions

- `rpc_gen.rs` is a 9k-line hand-maintained template — insert new types/
  calls carefully; the file must still compile (it's a Rust bin).
- `coalesce` signature change touches all callers — small blast radius,
  grep-able.
- Org-member removal must delete team rows *before* the sweep runs so
  `readable_repo_ids` sees post-removal truth — order matters inside the
  same handler.
- `repo.teams.*` in the repo crate shares the `organization_team_repos`
  table — cross-domain DB writes are fine (the façade is shared) but the
  audit target is `("repo", repo.id)` there vs `("team", team.id)` on the
  org side.
- Integration tests run on SQLite; dialect parity for the new SQL is
  CI's `dialect_probe` job.
