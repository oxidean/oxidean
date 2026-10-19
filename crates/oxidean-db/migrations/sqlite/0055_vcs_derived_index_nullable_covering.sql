-- u6 git-interop: import `active`/`cursor` marker rows exist before any
-- changeset is minted, so covering_changeset must admit NULL. SQLite cannot
-- drop a column constraint — full table rebuild per the repository_activity
-- precedent, FK + ON DELETE CASCADE preserved and
-- idx_vcs_derived_index_covering recreated (a rebuild drops secondary
-- indexes).
PRAGMA foreign_keys = OFF;
CREATE TABLE vcs_derived_index_new (
  repo_id             TEXT    NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  kind                TEXT    NOT NULL,
  index_key           TEXT    NOT NULL,
  payload             TEXT    NOT NULL,
  covering_changeset  TEXT    NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  updated_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (repo_id, kind, index_key)
);
INSERT INTO vcs_derived_index_new
  (repo_id, kind, index_key, payload, covering_changeset, updated_at)
  SELECT repo_id, kind, index_key, payload, covering_changeset, updated_at
    FROM vcs_derived_index;
DROP TABLE vcs_derived_index;
ALTER TABLE vcs_derived_index_new RENAME TO vcs_derived_index;
CREATE INDEX IF NOT EXISTS idx_vcs_derived_index_covering ON vcs_derived_index(covering_changeset);
PRAGMA foreign_keys = ON;
