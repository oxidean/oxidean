-- logical: 0054_action_artifacts — artifact storage for upload/download-artifact
-- (actions parity Phase 4). Content lives on disk under
-- OXIDEAN_ACTIONS_ARTIFACTS_DIR; rows track metadata + retention.

CREATE TABLE action_artifacts (
  id            TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  run_id        TEXT NOT NULL REFERENCES action_runs(id) ON DELETE CASCADE,
  job_id        TEXT NULL REFERENCES action_jobs(id) ON DELETE SET NULL,
  name          TEXT NOT NULL,
  storage_path  TEXT NOT NULL,
  size_bytes    INTEGER NOT NULL DEFAULT 0,
  digest_sha256 TEXT NOT NULL DEFAULT '',
  expires_at    TEXT NULL,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (run_id, name)
);

CREATE INDEX IF NOT EXISTS idx_action_artifacts_run ON action_artifacts(run_id);
CREATE INDEX IF NOT EXISTS idx_action_artifacts_expiry ON action_artifacts(expires_at);
