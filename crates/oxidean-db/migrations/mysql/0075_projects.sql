-- logical: 0075_projects — classic project boards (GitHub Projects API shape).
--
-- A project is a repo-scoped board with per-repo sequential numbers (same
-- shape as milestone_counters). Columns order within the board and cards
-- order within the column by `position` (0-based, re-packed on move/delete).
-- Cards are either notes (`note` set) or linked entities (`content_type` +
-- `content_id` point at an issue/pull row); `converted` swaps a note for an
-- issue link. GitHub's `project`/`project_column`/`project_card` event
-- payloads ride on these fields.
--
-- `projects_enabled` is the COL-13 unit flag — disabling never deletes data;
-- the `projects.*` RPC surface and Projects tab reject until re-enabled.

ALTER TABLE repositories ADD COLUMN projects_enabled TINYINT(1) NOT NULL DEFAULT 1;

CREATE TABLE IF NOT EXISTS projects (
  id          CHAR(36) PRIMARY KEY,
  repo_id     CHAR(36) NOT NULL,
  number      BIGINT NOT NULL,
  title       VARCHAR(255) NOT NULL,
  body        TEXT NOT NULL,
  state       VARCHAR(8) NOT NULL DEFAULT 'open',
  creator_id  CHAR(36) NULL,
  closed_at   DATETIME NULL,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_projects_repo_number (repo_id, number),
  CONSTRAINT fk_projects_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_projects_creator FOREIGN KEY (creator_id) REFERENCES users(id) ON DELETE SET NULL,
  CONSTRAINT chk_projects_state CHECK (state IN ('open', 'closed'))
);

CREATE INDEX idx_projects_repo_state
  ON projects (repo_id, state, updated_at);

CREATE TABLE IF NOT EXISTS project_counters (
  repo_id    CHAR(36) PRIMARY KEY,
  max_number BIGINT NOT NULL DEFAULT 0,
  CONSTRAINT fk_project_counters_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS project_columns (
  id          CHAR(36) PRIMARY KEY,
  project_id  CHAR(36) NOT NULL,
  name        VARCHAR(255) NOT NULL,
  position    BIGINT NOT NULL,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  CONSTRAINT fk_project_columns_project FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX idx_project_columns_project_position
  ON project_columns (project_id, position);

CREATE TABLE IF NOT EXISTS project_cards (
  id           CHAR(36) PRIMARY KEY,
  column_id    CHAR(36) NOT NULL,
  note         TEXT NULL,
  content_type VARCHAR(16) NULL,
  content_id   CHAR(36) NULL,
  position     BIGINT NOT NULL,
  archived     TINYINT(1) NOT NULL DEFAULT 0,
  creator_id   CHAR(36) NULL,
  created_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  CONSTRAINT fk_project_cards_column FOREIGN KEY (column_id) REFERENCES project_columns(id) ON DELETE CASCADE,
  CONSTRAINT fk_project_cards_creator FOREIGN KEY (creator_id) REFERENCES users(id) ON DELETE SET NULL,
  CONSTRAINT chk_project_cards_content_type CHECK (content_type IN ('issue', 'pull_request')),
  CONSTRAINT chk_project_cards_note_or_content CHECK (note IS NOT NULL OR content_id IS NOT NULL)
);

CREATE INDEX idx_project_cards_column_position
  ON project_cards (column_id, position);
CREATE INDEX idx_project_cards_content
  ON project_cards (content_type, content_id);
