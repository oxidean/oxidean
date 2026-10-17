-- logical: 0050_action_job_tokens — per-job Actions bearer tokens (OXIDEAN_TOKEN).
-- One live token per job (job_id UNIQUE); minted at claim, revoked on terminal
-- status / run cancel, hard expiry backstop at expires_at.

CREATE TABLE IF NOT EXISTS action_job_tokens (
  id            TEXT PRIMARY KEY,
  job_id        TEXT NOT NULL UNIQUE REFERENCES action_jobs(id) ON DELETE CASCADE,
  run_id        TEXT NOT NULL REFERENCES action_runs(id) ON DELETE CASCADE,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  permission    TEXT NOT NULL CHECK (permission IN ('read', 'write')),
  token_hash    TEXT NOT NULL UNIQUE,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  expires_at    TEXT NOT NULL,
  revoked_at    TEXT NULL
);

CREATE INDEX IF NOT EXISTS idx_action_job_tokens_run_id ON action_job_tokens(run_id);
