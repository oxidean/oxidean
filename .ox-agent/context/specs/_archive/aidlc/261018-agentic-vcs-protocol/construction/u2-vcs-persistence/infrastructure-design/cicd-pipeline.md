# CI/CD Pipeline — u2-vcs-persistence

## What this unit adds to the gates

| Change surface | CI gate(s) | How it's exercised |
|---|---|---|
| `crates/oxidean-db/src/vcs.rs` + repo_activity.rs edits | `nextest` (`cargo nextest run --workspace` — recorded verification command), `clippy`, `docs-check`, `dead-code-check` | unit tests on sqlite pool; compile sweep of consumers |
| `migrations/{postgres,mysql,sqlite}/0053_vcs_schema.sql` | `migration_parity` unit test (`migrate.rs:51-69`) + migration apply in API tests | filename parity assert; sqlx::migrate! embed + apply |
| `crates/oxidean-api/src/{repo/activity.rs,routes/feeds.rs,admin/users.rs}` de-wraps | nextest (activity/feed suites stay green), clippy | compile + existing repo_activity/feeds tests |
| `Cargo.lock` | none new (no new deps — TD2.6) | lockfile hygiene only |

## Lane mapping

- **nextest job** (ci.yml cargo lane): covers oxidean-db module + oxidean-api consumer tests on sqlite.
- **commit-lint job**: `db` scope exists for oxidean-db commits; `feat(db)`/`fix(db)` subjects.
- **dead-code-check** (knip + cargo machete + clippy -D dead_code): new façade methods must be reachable — u2's façade is consumed by u3+; interim dead-code risk mitigated by `#[allow(dead_code)]` on the module or unit tests touching every method (prefer tests — they gate correctness anyway).
- **docs-check**: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` — row-struct docs must be clean.

## Sequencing constraint

This unit must land before U3/U4 (they consume the façade). The `repository_activity` alteration is wire-adjacent: web feed tests (`web-octane` lane) keep passing because display fallbacks preserve output shape — verify `repo-activity-row.tsrx` guards still see `string|null` for the widened fields (U1 already widened the wire type).

## Rollback

`git revert` the commit; migration 0053 is additive + alter-only on repository_activity — a revert leaves the schema applied but unused (standard for additive migrations; no down-migration convention exists in this repo).
