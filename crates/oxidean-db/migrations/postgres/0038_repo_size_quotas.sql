-- logical: 0038_repo_size_quotas — bare-repo disk usage + git object size quota (GIT-25)
ALTER TABLE repositories ADD COLUMN IF NOT EXISTS size_bytes BIGINT NOT NULL DEFAULT 0;
ALTER TABLE repositories ADD COLUMN IF NOT EXISTS size_quota_bytes BIGINT;

CREATE TABLE IF NOT EXISTS instance_git_settings (
  id                    SMALLINT PRIMARY KEY CHECK (id = 1),
  repo_quota_bytes      BIGINT,
  updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO instance_git_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
