-- logical: 0053_vcs_schema — agentic VCS persistence (u2-vcs-persistence).
-- Global CAS objects + per-repo links, immutable changesets + file manifests,
-- CAS-mutated refs, derived index rows, git-oid import links, per-repo enable
-- flag. Also drops the repository_activity push_type CHECK (C4: vcs.* activity
-- kinds are recorded ref-free) and NULL-ables the four push columns.
-- MySQL: bounded VARCHAR/CHAR on every PK/UNIQUE column (3072B InnoDB budget).

CREATE TABLE IF NOT EXISTS vcs_repo_surface_flags (
  repo_id     CHAR(36)    PRIMARY KEY,
  enabled     TINYINT(1)  NOT NULL,
  enabled_by  CHAR(36)    NULL,
  enabled_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  CONSTRAINT fk_vcs_flags_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_vcs_flags_user FOREIGN KEY (enabled_by) REFERENCES users(id) ON DELETE SET NULL
) ENGINE=InnoDB;

-- Global CAS: hash is bare hex (the `sha256:` prefix is wire-only); row implies
-- bytes on disk. No repo_id — shared across repositories by design (FR1.5).
CREATE TABLE IF NOT EXISTS vcs_objects (
  hash        CHAR(64)    PRIMARY KEY,
  size        BIGINT      NOT NULL,
  created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS vcs_object_links (
  repo_id     CHAR(36)    NOT NULL,
  hash        CHAR(64)    NOT NULL,
  created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  PRIMARY KEY (repo_id, hash),
  CONSTRAINT fk_vcs_links_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_vcs_links_object FOREIGN KEY (hash) REFERENCES vcs_objects(hash) ON DELETE RESTRICT
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS vcs_changesets (
  id            VARCHAR(32)  PRIMARY KEY,
  repo_id       CHAR(36)    NOT NULL,
  parents       TEXT        NOT NULL,
  author        VARCHAR(255) NOT NULL,
  message       TEXT        NOT NULL,
  committed_at  DATETIME(6) NOT NULL,
  manifest_hash CHAR(64)    NOT NULL,
  idem_key      VARCHAR(255) NULL,
  created_at    DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  CONSTRAINT fk_vcs_changesets_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  -- NULLs stay distinct under UNIQUE: keyless inserts never collide here.
  UNIQUE KEY uk_vcs_changesets_repo_idem (repo_id, idem_key)
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS vcs_file_changes (
  changeset_id VARCHAR(32)  NOT NULL,
  path         VARCHAR(512) NOT NULL,
  kind         VARCHAR(16)  NOT NULL,
  mode         INT          NOT NULL,
  old_path     VARCHAR(512) NULL,
  blob_hash    CHAR(64)     NULL,
  PRIMARY KEY (changeset_id, path),
  CONSTRAINT fk_vcs_file_changes_cs FOREIGN KEY (changeset_id) REFERENCES vcs_changesets(id) ON DELETE CASCADE,
  CONSTRAINT fk_vcs_file_changes_blob FOREIGN KEY (blob_hash) REFERENCES vcs_objects(hash) ON DELETE RESTRICT
) ENGINE=InnoDB;

-- The only mutable entity: changeset_id is CAS-updated; delete removes the row
-- (absent refs, not tombstones).
CREATE TABLE IF NOT EXISTS vcs_refs (
  repo_id      CHAR(36)     NOT NULL,
  name         VARCHAR(255) NOT NULL,
  changeset_id VARCHAR(32)  NULL,
  updated_at   DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  PRIMARY KEY (repo_id, name),
  CONSTRAINT fk_vcs_refs_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_vcs_refs_cs FOREIGN KEY (changeset_id) REFERENCES vcs_changesets(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS vcs_derived_index (
  repo_id             CHAR(36)     NOT NULL,
  kind                VARCHAR(64)  NOT NULL,
  index_key           VARCHAR(255) NOT NULL,
  payload             TEXT         NOT NULL,
  covering_changeset  VARCHAR(32)  NOT NULL,
  updated_at          DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  PRIMARY KEY (repo_id, kind, index_key),
  CONSTRAINT fk_vcs_didx_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_vcs_didx_cs FOREIGN KEY (covering_changeset) REFERENCES vcs_changesets(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS vcs_import_links (
  repo_id      CHAR(36)    NOT NULL,
  git_oid      CHAR(64)    NOT NULL,
  changeset_id VARCHAR(32)  NOT NULL,
  created_at   DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  PRIMARY KEY (repo_id, git_oid),
  CONSTRAINT fk_vcs_ilinks_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_vcs_ilinks_cs FOREIGN KEY (changeset_id) REFERENCES vcs_changesets(id) ON DELETE CASCADE
) ENGINE=InnoDB;

-- Secondary indexes: GC anti-join on link hash, grace-bound object scan,
-- repo-scoped changeset queries, and FK-side cascade checks.
CREATE INDEX idx_vcs_object_links_hash ON vcs_object_links(hash);
CREATE INDEX idx_vcs_objects_created_at ON vcs_objects(created_at);
CREATE INDEX idx_vcs_changesets_repo ON vcs_changesets(repo_id);
CREATE INDEX idx_vcs_refs_changeset ON vcs_refs(changeset_id);
CREATE INDEX idx_vcs_file_changes_blob ON vcs_file_changes(blob_hash);
CREATE INDEX idx_vcs_import_links_changeset ON vcs_import_links(changeset_id);
CREATE INDEX idx_vcs_derived_index_covering ON vcs_derived_index(covering_changeset);

-- C4: drop the push_type allow-list entirely (live CHECK includes
-- branch_rename since 0025) and let ref_name/before_oid/after_oid/
-- commits_count carry NULL for ref-free vcs.* activity rows.
ALTER TABLE repository_activity
  DROP CHECK chk_repository_activity_push_type;

ALTER TABLE repository_activity MODIFY COLUMN ref_name VARCHAR(512) NULL;
ALTER TABLE repository_activity MODIFY COLUMN before_oid VARCHAR(64) NULL DEFAULT '';
ALTER TABLE repository_activity MODIFY COLUMN after_oid VARCHAR(64) NULL DEFAULT '';
ALTER TABLE repository_activity MODIFY COLUMN commits_count BIGINT NULL DEFAULT 0;
