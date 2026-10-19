# Entities — u2-vcs-persistence

Persistence-façade unit: eight new row entities owned by `oxidean-db` (sole dialect-SQL site), plus the `repository_activity` alteration and the `Database`-by-value façade. One sanctioned crate-boundary exception: de-wrap the three `RepoActivityRow` consumers in `oxidean-api` (`repo/activity.rs`, `routes/feeds.rs`, `admin/users.rs`) — pinned by u1's ownership split, no route logic changes.

**Type conventions (house pins):** ids `CHAR(36)` on mysql / `TEXT` pg+sqlite; `changeset_id` = `cs_`+26-ULID → `CHAR(29)`/`VARCHAR(32)`; `hash`/`git_oid` = bare 40/64-hex → `CHAR(64)`/mysql, `TEXT` elsewhere (object hashes stored bare — `sha256:` prefix is wire-only, `vcs_types.rs:169`); `parents`/`payload` = `TEXT` JSON (serde at boundary — 0047/webhooks precedent); timestamps `TIMESTAMPTZ`/`DATETIME(6)`/`TEXT`. mysql PK budget 3072B → bounded `VARCHAR` for `path`(512)/`index_key`(255)/`name`(255)/`kind`(64)/`idem_key`(255). Column named `key` is mysql-reserved → `index_key` on all dialects.

```yaml
entities:
  - name: RepoSurfaceFlag
    description: "per-repo VCS enablement flag — absent row = never-enabled (denies vcs.* as vcs.disabled); present+disabled = admin-disabled"
    identifier: repo_id
    attributes:
      - {name: repo_id, type: "id FK repositories(id) CASCADE", required: true}
      - {name: enabled, type: "BOOL NOT NULL", required: true}
      - {name: enabled_by, type: "id NULL (user)", required: false}
      - {name: enabled_at, type: "timestamp NOT NULL DEFAULT now()", required: true}
    constraints: ["PK repo_id", "upsert on repo_enable"]
    relationships: "1:1 repositories"

  - name: ObjectEntry
    description: "GLOBAL CAS row keyed by hash (shared CAS FR1.5); bytes on disk; row implies file"
    identifier: hash
    attributes:
      - {name: hash, type: "hash-id PK (bare hex)", required: true}
      - {name: size, type: "BIGINT NOT NULL", required: true}
      - {name: created_at, type: "timestamp NOT NULL DEFAULT now()", required: true}
    constraints: ["insert_if_absent→bool (rows_affected idiom: 1=stored,0=dedup — u3 Stored|Dedup)", "list_orphaned_before(grace_ts) anti-join"]
    relationships: "1:N ObjectLink"

  - name: ObjectLink
    description: "per-repo reference/quota attribution for a global object — inserted inside create_tx"
    identifier: "repo_id + hash"
    attributes:
      - {name: repo_id, type: "id FK repositories(id) CASCADE", required: true}
      - {name: hash, type: "hash-id FK vcs_objects(hash)", required: true}
      - {name: created_at, type: "timestamp NOT NULL DEFAULT now()", required: true}
    constraints: ["PK (repo_id,hash)", "INSERT-IF-ABSENT (ON CONFLICT DO NOTHING / INSERT OR IGNORE / INSERT IGNORE) — later changesets re-referencing a blob MUST NOT collide (revert/merge/unchanged-path manifests)", "live_hashes() feeds GC; invariant: every manifest blob has a link in same repo (u3/u4 own; FK is the protective assertion)"]
    relationships: "N:1 ObjectEntry; N:1 repositories"

  - name: Changeset
    description: "immutable server-authored record — deterministic id; PK column named `id` (house convention)"
    identifier: id
    attributes:
      - {name: id, type: "changeset-id PK (CHAR(29)/VARCHAR(32))", required: true}
      - {name: repo_id, type: "id FK repositories(id) CASCADE", required: true}
      - {name: parents, type: "TEXT NOT NULL — JSON array of parent ids (parse at boundary)", required: true}
      - {name: author, type: "TEXT/VARCHAR(255) NOT NULL (canonical 'Name <email>')", required: true}
      - {name: message, type: "TEXT NOT NULL", required: true}
      - {name: committed_at, type: "timestamp NOT NULL — caller-supplied; pg bind needs ::timestamptz cast (repo_activity.rs:336 precedent)", required: true}
      - {name: manifest_hash, type: "hash-id NOT NULL", required: true}
      - {name: idem_key, type: "VARCHAR(255) NULL — UNIQUE(repo_id,idem_key)", required: false}
      - {name: created_at, type: "timestamp NOT NULL DEFAULT now()", required: true}
    constraints: ["idempotent resubmit: INSERT-IF-ABSENT-on-idem or unique-race → re-SELECT existing (star_repository pattern — never error-string matching; pg/mysql/sqlite violation texts differ)"]
    relationships: "1:N FileChange; head of Ref"

  - name: FileChange
    description: "manifest entry — one per changed path"
    identifier: "changeset_id + path"
    attributes:
      - {name: changeset_id, type: "id FK vcs_changesets(id) CASCADE", required: true}
      - {name: path, type: "VARCHAR(512)/TEXT NOT NULL", required: true}
      - {name: kind, type: "VARCHAR(16)/TEXT NOT NULL (add|modify|delete|rename)", required: true}
      - {name: mode, type: "INT NOT NULL", required: true}
      - {name: old_path, type: "VARCHAR(512)/TEXT NULL (rename)", required: false}
      - {name: blob_hash, type: "hash-id NULL — FK vcs_objects(hash); delete carries none", required: false}
    constraints: ["PK (changeset_id,path)", "non-delete ⇒ blob_hash NOT NULL (façade asserts before insert)"]
    relationships: "N:1 Changeset; N:1 ObjectEntry"

  - name: Ref
    description: "named head pointer — the ONLY mutable entity; strict CAS; name stored bare (main, agent/task-77 — no refs/heads/ prefix)"
    identifier: "repo_id + name"
    attributes:
      - {name: repo_id, type: "id FK repositories(id) CASCADE", required: true}
      - {name: name, type: "VARCHAR(255)/TEXT NOT NULL", required: true}
      - {name: changeset_id, type: "id NULL — FK vcs_changesets(id)", required: false}
      - {name: updated_at, type: "timestamp NOT NULL DEFAULT now()", required: true}
    constraints: ["PK (repo_id,name)", "cas_update returns CasOutcome{Applied(RefRow)|Conflict{actual:Option<String>}} — Result<_,String> can't carry `actual` (C1 data.actual)", "null-safe equality: pg/sqlite `IS NOT DISTINCT FROM`, mysql `<=>`; mysql has no RETURNING → rows_affected() then SELECT actual (stars.rs precedent)", "expected_old NULL+absent ⇒ create path: pg/sqlite INSERT..ON CONFLICT..DO UPDATE..WHERE, mysql UPDATE→INSERT in tx", "delete = row DELETE (AC5.2.1 wants absent refs, not tombstones)"]
    relationships: "N:1 Changeset"

  - name: DerivedIndex
    description: "incremental derived rows (walk/diff/blame/path-history/merge-base) — merge-safe upserts"
    identifier: "repo_id + kind + index_key"
    attributes:
      - {name: repo_id, type: "id FK repositories(id) CASCADE", required: true}
      - {name: kind, type: "VARCHAR(64)/TEXT NOT NULL", required: true}
      - {name: index_key, type: "VARCHAR(255)/TEXT NOT NULL (mysql-reserved `key` avoided)", required: true}
      - {name: payload, type: "TEXT NOT NULL — JSON (parse at boundary)", required: true}
      - {name: covering_changeset, type: "id FK vcs_changesets(id)", required: true}
      - {name: updated_at, type: "timestamp NOT NULL DEFAULT now()", required: true}
    constraints: ["PK (repo_id,kind,index_key)", "ON CONFLICT merge upsert — concurrent same-parent safe (AC2.3.1)", "query(kind,index_key,cursor) paginated"]
    relationships: "N:1 Changeset"

  - name: ImportLink
    description: "git_oid ↔ changeset_id mapping — import idempotency + export walks"
    identifier: "repo_id + git_oid"
    attributes:
      - {name: repo_id, type: "id FK repositories(id) CASCADE", required: true}
      - {name: git_oid, type: "CHAR(40)/TEXT NOT NULL (bare hex)", required: true}
      - {name: changeset_id, type: "id NOT NULL FK vcs_changesets(id) CASCADE", required: true}
      - {name: created_at, type: "timestamp NOT NULL DEFAULT now()", required: true}
    constraints: ["PK (repo_id,git_oid)", "lookup → changeset_id", "cursor_page for export"]
    relationships: "N:1 Changeset"

  - name: VcsPersistenceFacade
    description: "Database-by-value method surface (C5) — require_pool → db.not_configured"
    attributes:
      - {name: repo_surface_flag, type: "{get->Option<row>, upsert}", required: true}
      - {name: object_entry, type: "{get_by_hash->Option, insert_if_absent->bool, list_orphaned_before(grace_ts)}", required: true}
      - {name: object_link, type: "{insert_if_absent, count_for_repo->{object_count:i64,storage_bytes:i64} (SUM(size) JOIN — lfs.rs:398 precedent), live_hashes}", required: true}
      - {name: changeset, type: "{create_tx(cs,file_changes,object_links,index_upserts)->id, get->Option, parents_of->Vec<id>, find_by_idempotency->Option, count_for_repo->i64 (vcs.status changeset_count — C1 pin)}", required: true}
      - {name: ref, type: "{list->page, cas_update->CasOutcome, delete}", required: true}
      - {name: derived_index, type: "{upsert_rows, query(kind,index_key,cursor)}", required: true}
      - {name: import_link, type: "{insert, lookup, cursor_page}", required: true}
      - {name: repo_activity, type: "insert_repo_activity — Option<> push cols per C4", required: true}
    relationships: "impl on Database; consumed by U3/U4/U6/U7"
```

## indexes (secondary — GC/joins/cascades)

- `vcs_object_links(hash)` — `list_orphaned_before` anti-join (lfs idx_lfs_object_links_oid precedent).
- `vcs_changesets(repo_id)` — repo-scoped queries + cascade checks (house indexes FK cols).
- `vcs_objects(created_at)` — grace-bound GC scan.
- `vcs_refs(changeset_id)`, `vcs_file_changes(blob_hash)`, `vcs_import_links(changeset_id)`, `vcs_derived_index(covering_changeset)` — FK-side cascade checks.

## repository_activity alteration (0053) — per-dialect recipes

Live CHECK includes `branch_rename` (0025 ×3) — drop it entirely, not the stale 4-kind literal.

- **postgres**: `DROP CONSTRAINT repository_activity_push_type_check` + `ALTER COLUMN ref_name|before_oid|after_oid|commits_count DROP NOT NULL` ×4.
- **mysql**: `DROP CHECK chk_repository_activity_push_type` + `MODIFY COLUMN … NULL` (0032:11-25 precedent).
- **sqlite**: full rebuild per 0025 (`PRAGMA foreign_keys=OFF` → `repository_activity_new` (no CHECK, NULL cols) → copy → DROP old → RENAME → recreate `idx_repository_activity_repo_created` + `idx_repository_activity_repo_type_created` → `PRAGMA ON`).

## RepoActivityRow read-side (part of this unit — u1 pinned it)

- `RepoActivityRow` 4 fields → `Option` (ref_name/before_oid/after_oid String→Option<String>, commits_count i64→Option<i64>); `map_activity!` → `try_get::<Option<…>>` (keep i32→i64 fallback); `InsertRepoActivity` → Option<>.
- oxidean-api de-wraps (compile sweep, no logic change): `repo/activity.rs:367-372` row_to_item; `routes/feeds.rs` (~8 sites incl. `short_ref(&row.ref_name)`, `commits_count` comparisons, test literals at :546/:571 — fallback "" / 0 for display paths); `admin/users.rs:335-336` (fields already Option — pass through).
