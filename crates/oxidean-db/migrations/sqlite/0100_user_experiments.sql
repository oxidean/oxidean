-- logical: 0100_user_experiments — per-user experiment flags (WI-0001).
-- Row present = flag enabled for that user; keys are validated against the
-- oxidean-core EXPERIMENTS registry, never trusted from the wire. Flag
-- retirement deletes the registry entry + gate sites; stale rows are inert.

CREATE TABLE IF NOT EXISTS user_experiments (
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  flag_key   TEXT NOT NULL,
  enabled_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (user_id, flag_key)
);
