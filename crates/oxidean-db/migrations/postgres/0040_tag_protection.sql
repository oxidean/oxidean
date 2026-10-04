-- logical: 0033_tag_protection — protected tag rulesets (GIT-21)

CREATE TABLE IF NOT EXISTS tag_protection_rules (
  id             TEXT PRIMARY KEY,
  repo_id        TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  pattern        TEXT NOT NULL,
  allow_create   BOOLEAN NOT NULL DEFAULT false,
  allow_update   BOOLEAN NOT NULL DEFAULT false,
  allow_delete   BOOLEAN NOT NULL DEFAULT false,
  enforce_admins BOOLEAN NOT NULL DEFAULT false,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tag_protection_rules_repo
  ON tag_protection_rules(repo_id);
