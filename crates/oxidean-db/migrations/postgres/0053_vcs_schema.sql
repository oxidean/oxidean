-- logical: 0053_vcs_schema — agentic VCS persistence (u2-vcs-persistence).
-- Global CAS objects + per-repo links, immutable changesets + file manifests,
-- CAS-mutated refs, derived index rows, git-oid import links, per-repo enable
-- flag. Also drops the repository_activity push_type CHECK (C4: vcs.* activity
-- kinds are recorded ref-free) and NULL-ables the four push columns.

CREATE TABLE IF NOT EXISTS vcs_repo_surface_flags (
  repo_id     TEXT        PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  enabled     BOOLEAN     NOT NULL,
  enabled_by  TEXT        NULL REFERENCES users(id) ON DELETE SET NULL,
  enabled_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Global CAS: hash is bare hex (the `sha256:` prefix is wire-only); row implies
-- bytes on disk. No repo_id — shared across repositories by design (FR1.5).
CREATE TABLE IF NOT EXISTS vcs_objects (
  hash        TEXT        PRIMARY KEY,
  size        BIGINT      NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS vcs_object_links (
  repo_id     TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  hash        TEXT        NOT NULL REFERENCES vcs_objects(hash),
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (repo_id, hash)
);

CREATE TABLE IF NOT EXISTS vcs_changesets (
  id            CHAR(29)    PRIMARY KEY,
  repo_id       TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  parents       TEXT        NOT NULL,
  author        TEXT        NOT NULL,
  message       TEXT        NOT NULL,
  committed_at  TIMESTAMPTZ NOT NULL,
  manifest_hash TEXT        NOT NULL,
  idem_key      TEXT        NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Idempotency key is optional; NULLs stay distinct under UNIQUE (resubmits
-- carrying a key dedup, keyless inserts never collide here).
CREATE UNIQUE INDEX IF NOT EXISTS uk_vcs_changesets_repo_idem
  ON vcs_changesets(repo_id, idem_key);

CREATE TABLE IF NOT EXISTS vcs_file_changes (
  changeset_id CHAR(29) NOT NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  path         TEXT    NOT NULL,
  kind         TEXT    NOT NULL,
  mode         INTEGER NOT NULL,
  old_path     TEXT    NULL,
  blob_hash    TEXT    NULL REFERENCES vcs_objects(hash),
  PRIMARY KEY (changeset_id, path)
);

-- The only mutable entity: changeset_id is CAS-updated; delete removes the row
-- (absent refs, not tombstones).
CREATE TABLE IF NOT EXISTS vcs_refs (
  repo_id      TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  name         TEXT        NOT NULL,
  changeset_id CHAR(29)    NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (repo_id, name)
);

CREATE TABLE IF NOT EXISTS vcs_derived_index (
  repo_id             TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  kind                TEXT        NOT NULL,
  index_key           TEXT        NOT NULL,
  payload             TEXT        NOT NULL,
  covering_changeset  CHAR(29)    NOT NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (repo_id, kind, index_key)
);

CREATE TABLE IF NOT EXISTS vcs_import_links (
  repo_id      TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  git_oid      TEXT        NOT NULL,
  changeset_id CHAR(29)    NOT NULL REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
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

-- C4: drop the push_type allow-list entirely (live CHECK includes
-- branch_rename since 0025) and let ref_name/before_oid/after_oid/
-- commits_count carry NULL for ref-free vcs.* activity rows.
ALTER TABLE repository_activity
  DROP CONSTRAINT IF EXISTS repository_activity_push_type_check;

ALTER TABLE repository_activity ALTER COLUMN ref_name DROP NOT NULL;
ALTER TABLE repository_activity ALTER COLUMN before_oid DROP NOT NULL;
ALTER TABLE repository_activity ALTER COLUMN after_oid DROP NOT NULL;
ALTER TABLE repository_activity ALTER COLUMN commits_count DROP NOT NULL;
