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

ALTER TABLE repositories ADD COLUMN projects_enabled INTEGER NOT NULL DEFAULT 1;

CREATE TABLE IF NOT EXISTS projects (
  id          TEXT PRIMARY KEY,
  repo_id     TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  number      INTEGER NOT NULL,
  title       TEXT NOT NULL,
  body        TEXT NOT NULL DEFAULT '',
  state       TEXT NOT NULL DEFAULT 'open'
    CHECK (state IN ('open', 'closed')),
  creator_id  TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  closed_at   TEXT NULL,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (repo_id, number)
);

CREATE INDEX IF NOT EXISTS idx_projects_repo_state
  ON projects (repo_id, state, updated_at DESC);

CREATE TABLE IF NOT EXISTS project_counters (
  repo_id    TEXT PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  max_number INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS project_columns (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  position    INTEGER NOT NULL,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_project_columns_project_position
  ON project_columns (project_id, position);

CREATE TABLE IF NOT EXISTS project_cards (
  id           TEXT PRIMARY KEY,
  column_id    TEXT NOT NULL REFERENCES project_columns(id) ON DELETE CASCADE,
  note         TEXT NULL,
  content_type TEXT NULL CHECK (content_type IN ('issue', 'pull_request')),
  content_id   TEXT NULL,
  position     INTEGER NOT NULL,
  archived     INTEGER NOT NULL DEFAULT 0,
  creator_id   TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  CHECK (note IS NOT NULL OR content_id IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_project_cards_column_position
  ON project_cards (column_id, position);
CREATE INDEX IF NOT EXISTS idx_project_cards_content
  ON project_cards (content_type, content_id);
