-- logical: 0085_oxpilot — Oxpilot agentic-chat sessions and transcripts.
--
-- `oxpilot_sessions` carries one `/oxpilot` conversation per row: user-owned,
-- bound to a forge repository + branch. `oxpilot_messages` is the per-session
-- ordered transcript (`seq` monotonic per session); `client_msg_id` is the
-- sender-supplied idempotency key on user rows.
--
-- Every read/write is caller-scoped by `sessions.user_id` — cross-user access
-- resolves `not_found` (anti-enumeration). Content is secrets-redacted before
-- insert by the API layer; the table itself holds only redacted text.

CREATE TABLE IF NOT EXISTS oxpilot_sessions (
  id               TEXT PRIMARY KEY,
  user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  repo_owner       TEXT NOT NULL,
  repo_name        TEXT NOT NULL,
  repo_id          TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  branch           TEXT NOT NULL,
  goal             TEXT NULL,
  title            TEXT NOT NULL DEFAULT 'Untitled session',
  status           TEXT NOT NULL DEFAULT 'active',
  created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  last_activity_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_oxpilot_sessions_user_activity
  ON oxpilot_sessions (user_id, last_activity_at DESC);

CREATE TABLE IF NOT EXISTS oxpilot_messages (
  id            TEXT PRIMARY KEY,
  session_id    TEXT NOT NULL REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  seq           INTEGER NOT NULL,
  role          TEXT NOT NULL,
  content       TEXT NOT NULL DEFAULT '',
  client_msg_id TEXT NULL,
  status        TEXT NOT NULL DEFAULT 'complete',
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (session_id, seq),
  UNIQUE (session_id, client_msg_id)
);

CREATE INDEX IF NOT EXISTS idx_oxpilot_messages_session_seq
  ON oxpilot_messages (session_id, seq);
