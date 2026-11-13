-- logical: 0104_saved_replies — per-user saved comment replies (GitHub
-- "saved replies"). One row per (user, reply); `name` is the picker label,
-- `body` the markdown inserted into a composer. User-scoped — no repo/org
-- binding in v1.

CREATE TABLE IF NOT EXISTS saved_replies (
  id         TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name       TEXT NOT NULL,
  body       TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
CREATE INDEX IF NOT EXISTS idx_saved_replies_user ON saved_replies(user_id, updated_at DESC);
