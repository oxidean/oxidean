-- logical: 0053_vcs_schema — agentic VCS persistence (u2-vcs-persistence).
-- Global CAS objects + per-repo links, immutable changesets + file manifests,
-- CAS-mutated refs, derived index rows, git-oid import links, per-repo enable
-- flag. Also drops the repository_activity push_type CHECK (C4: vcs.* activity
-- kinds are recorded ref-free) and NULL-ables the four push columns — sqlite
-- has no DROP CONSTRAINT/DROP NOT NULL, so the table is rebuilt per 0025.

CREATE TABLE IF NOT EXISTS vcs_repo_surface_flags (
  repo_id     TEXT    PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  enabled     INTEGER NOT NULL,
  enabled_by  TEXT    NULL REFERENCES users(id) ON DELETE SET NULL,
  enabled_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

-- Global CAS: hash is bare hex (the `sha256:` prefix is wire-only); row implies
-- bytes on disk. No repo_id — shared across repositories by design (FR1.5).
CREATE TABLE IF NOT EXISTS vcs_objects (
  hash        TEXT    PRIMARY KEY,
  size        INTEGER NOT NULL,
  created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE TABLE IF NOT EXISTS vcs_object_links (
  repo_id     TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  hash        TEXT    NOT NULL REFERENCES vcs_objects(hash) ON DELETE NO ACTION,
  created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (repo_id, hash)
);

CREATE TABLE IF NOT EXISTS vcs_changesets (
  id            TEXT    PRIMARY KEY,
  repo_id       TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  parents       TEXT    NOT NULL,
  author        TEXT    NOT NULL,
  message       TEXT    NOT NULL,
  committed_at  TEXT    NOT NULL,
  manifest_hash TEXT    NOT NULL,
  idem_key      TEXT    NULL,
  created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

-- Idempotency key is optional; NULLs stay distinct under UNIQUE (resubmits
-- carrying a key dedup, keyless inserts never collide here).
CREATE UNIQUE INDEX IF NOT EXISTS uk_vcs_changesets_repo_idem
  ON vcs_changesets(repo_id, idem_key);

CREATE TABLE IF NOT EXISTS vcs_file_changes (
  changeset_id TEXT    NOT NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  path         TEXT    NOT NULL,
  kind         TEXT    NOT NULL,
  mode         INTEGER NOT NULL,
  old_path     TEXT    NULL,
  blob_hash    TEXT    NULL REFERENCES vcs_objects(hash) ON DELETE NO ACTION,
  PRIMARY KEY (changeset_id, path)
);

-- The only mutable entity: changeset_id is CAS-updated; delete removes the row
-- (absent refs, not tombstones).
CREATE TABLE IF NOT EXISTS vcs_refs (
  repo_id      TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  name         TEXT    NOT NULL,
  changeset_id TEXT    NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (repo_id, name)
);

CREATE TABLE IF NOT EXISTS vcs_derived_index (
  repo_id             TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  kind                TEXT    NOT NULL,
  index_key           TEXT    NOT NULL,
  payload             TEXT    NOT NULL,
  covering_changeset  TEXT    NOT NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  updated_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (repo_id, kind, index_key)
);

CREATE TABLE IF NOT EXISTS vcs_import_links (
  repo_id      TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  git_oid      TEXT    NOT NULL,
  changeset_id TEXT    NOT NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (repo_id, git_oid)
);

-- Secondary indexes: GC anti-join on link hash, grace-bound object scan,
-- repo-scoped changeset queries, and FK-side cascade checks.
CREATE INDEX IF NOT EXISTS idx_vcs_object_links_hash ON vcs_object_links(hash);
CREATE INDEX IF NOT EXISTS idx_vcs_objects_created_at ON vcs_objects(created_at);
CREATE INDEX IF NOT EXISTS idx_vcs_changesets_repo ON vcs_changesets(repo_id);
CREATE INDEX IF NOT EXISTS idx_vcs_refs_changeset ON vcs_refs(changeset_id);
CREATE INDEX IF NOT EXISTS idx_vcs_file_changes_blob ON vcs_file_changes(blob_hash);
CREATE INDEX IF NOT EXISTS idx_vcs_import_links_changeset ON vcs_import_links(changeset_id);
CREATE INDEX IF NOT EXISTS idx_vcs_derived_index_covering ON vcs_derived_index(covering_changeset);

-- C4: repository_activity loses the push_type CHECK (which has covered
-- branch_rename since 0025) and the four push columns become NULL — full
-- rebuild per the 0025 precedent, both indexes recreated.
PRAGMA foreign_keys = OFF;
CREATE TABLE repository_activity_new (
  id              TEXT PRIMARY KEY,
  repository_id   TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  actor_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  push_type       TEXT NOT NULL,
  ref_name        TEXT NULL,
  before_oid      TEXT NULL DEFAULT '',
  after_oid       TEXT NULL DEFAULT '',
  commits_count   INTEGER NULL DEFAULT 0,
  commit_message  TEXT NULL,
  pr_number       INTEGER NULL,
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
INSERT INTO repository_activity_new
  SELECT id, repository_id, actor_id, push_type, ref_name, before_oid, after_oid,
         commits_count, commit_message, pr_number, created_at
  FROM repository_activity;
DROP TABLE repository_activity;
ALTER TABLE repository_activity_new RENAME TO repository_activity;
CREATE INDEX IF NOT EXISTS idx_repository_activity_repo_created
  ON repository_activity(repository_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_repository_activity_repo_type_created
  ON repository_activity(repository_id, push_type, created_at DESC);
PRAGMA foreign_keys = ON;
