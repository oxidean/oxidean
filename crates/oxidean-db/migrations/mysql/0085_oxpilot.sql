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
  id               CHAR(36) PRIMARY KEY,
  user_id          CHAR(36) NOT NULL,
  repo_owner       VARCHAR(255) NOT NULL,
  repo_name        VARCHAR(255) NOT NULL,
  repo_id          CHAR(36) NOT NULL,
  branch           VARCHAR(255) NOT NULL,
  goal             TEXT NULL,
  title            VARCHAR(255) NOT NULL DEFAULT 'Untitled session',
  status           VARCHAR(32) NOT NULL DEFAULT 'active',
  created_at       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_activity_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_oxpilot_sessions_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
  CONSTRAINT fk_oxpilot_sessions_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE INDEX idx_oxpilot_sessions_user_activity
  ON oxpilot_sessions (user_id, last_activity_at DESC);

CREATE TABLE IF NOT EXISTS oxpilot_messages (
  id            CHAR(36) PRIMARY KEY,
  session_id    CHAR(36) NOT NULL,
  seq           BIGINT NOT NULL,
  role          VARCHAR(32) NOT NULL,
  content       MEDIUMTEXT NOT NULL,
  client_msg_id VARCHAR(255) NULL,
  status        VARCHAR(32) NOT NULL DEFAULT 'complete',
  created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_oxpilot_messages_session FOREIGN KEY (session_id) REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  UNIQUE KEY uq_oxpilot_messages_session_seq (session_id, seq),
  UNIQUE KEY uq_oxpilot_messages_session_client (session_id, client_msg_id)
);

CREATE INDEX idx_oxpilot_messages_session_seq
  ON oxpilot_messages (session_id, seq);
