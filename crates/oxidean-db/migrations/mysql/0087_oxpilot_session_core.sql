-- logical: 0087_oxpilot_session_core — the U2 session-core tables: durable
-- tool-call cards, the mid-stream follow-up queue, proposed diffs, and
-- per-day usage counters.
--
-- `oxpilot_tool_calls` is the durable form of the C2 SSE `tool` frame —
-- keyed by the owning assistant `message_id` plus a per-call `call_id` so
-- repeated same-tool invocations under one message persist distinctly.
-- `seq` is a positional copy of the owning message's seq (never minted
-- here); `call_index` orders cards inside that message.
--
-- `oxpilot_queued_messages` holds user follow-ups enqueued while a reply
-- streams (US1.8). `queue_seq` is per-session monotonic; `client_msg_id`
-- shares the union-dedupe key with `oxpilot_messages` and is carried into
-- the appended row by the single-transaction drain.
--
-- `oxpilot_proposals` is the reviewable diff artifact — `diff_body` holds
-- canonical unified-diff text; C1's `files[{path,patch}]` derives by
-- splitting on 'diff --git' headers at read time.
--
-- `oxpilot_usage_counters` is the standalone audit-grade rollup for
-- US5.2 — one row per (scope, scope_key, window), window the UTC ISO day.
-- It carries NO session/user FK on purpose: counters survive session and
-- user deletes.
--
-- All three session-child tables cascade with `oxpilot_sessions` so the
-- C8 hook-then-cascade delete is a single statement. Body-bearing fields
-- (content, args_summary, output_summary, title, diff_body) arrive already
-- secrets-redacted from the service boundary.

CREATE TABLE IF NOT EXISTS oxpilot_tool_calls (
  id             CHAR(36) PRIMARY KEY,
  session_id     CHAR(36) NOT NULL,
  message_id     CHAR(36) NOT NULL,
  call_id        VARCHAR(255) NOT NULL,
  seq            BIGINT NOT NULL,
  call_index     BIGINT NOT NULL,
  tool           VARCHAR(255) NOT NULL,
  args_summary   MEDIUMTEXT NOT NULL,
  output_summary MEDIUMTEXT NOT NULL,
  status         VARCHAR(32) NOT NULL,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_oxpilot_tool_calls_session FOREIGN KEY (session_id) REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  UNIQUE KEY uq_oxpilot_tool_calls_session_call (session_id, call_id)
);

CREATE INDEX idx_oxpilot_tool_calls_message
  ON oxpilot_tool_calls (session_id, message_id, call_index);

CREATE TABLE IF NOT EXISTS oxpilot_queued_messages (
  id            CHAR(36) PRIMARY KEY,
  session_id    CHAR(36) NOT NULL,
  queue_seq     BIGINT NOT NULL,
  content       MEDIUMTEXT NOT NULL,
  client_msg_id VARCHAR(255) NULL,
  created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_oxpilot_queued_messages_session FOREIGN KEY (session_id) REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  UNIQUE KEY uq_oxpilot_queued_messages_session_seq (session_id, queue_seq),
  UNIQUE KEY uq_oxpilot_queued_messages_session_client (session_id, client_msg_id)
);

CREATE INDEX idx_oxpilot_queued_messages_session
  ON oxpilot_queued_messages (session_id);

CREATE TABLE IF NOT EXISTS oxpilot_proposals (
  id         CHAR(36) PRIMARY KEY,
  session_id CHAR(36) NOT NULL,
  title      VARCHAR(255) NOT NULL,
  base       VARCHAR(255) NOT NULL,
  diff_body  MEDIUMTEXT NOT NULL,
  status     VARCHAR(32) NOT NULL DEFAULT 'pending',
  pr_ref     VARCHAR(255) NULL,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_oxpilot_proposals_session FOREIGN KEY (session_id) REFERENCES oxpilot_sessions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS oxpilot_usage_counters (
  id         CHAR(36) PRIMARY KEY,
  scope      VARCHAR(32) NOT NULL,
  scope_key  VARCHAR(255) NOT NULL,
  window     VARCHAR(16) NOT NULL,
  requests   BIGINT NOT NULL DEFAULT 0,
  tokens     BIGINT NOT NULL DEFAULT 0,
  updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uq_oxpilot_usage_counters_key (scope, scope_key, window)
);
