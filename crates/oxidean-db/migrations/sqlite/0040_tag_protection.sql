-- logical: 0033_tag_protection — protected tag rulesets (GIT-21)
-- Dialect SQL only.

CREATE TABLE IF NOT EXISTS tag_protection_rules (
  id             TEXT PRIMARY KEY,
  repo_id        TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  pattern        TEXT NOT NULL,
  allow_create   INTEGER NOT NULL DEFAULT 0,
  allow_update   INTEGER NOT NULL DEFAULT 0,
  allow_delete   INTEGER NOT NULL DEFAULT 0,
  enforce_admins INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_tag_protection_rules_repo
  ON tag_protection_rules(repo_id);
