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
  id             TEXT PRIMARY KEY,
  session_id     TEXT NOT NULL REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  message_id     TEXT NOT NULL,
  call_id        TEXT NOT NULL,
  seq            BIGINT NOT NULL,
  call_index     BIGINT NOT NULL,
  tool           TEXT NOT NULL,
  args_summary   TEXT NOT NULL DEFAULT '',
  output_summary TEXT NOT NULL DEFAULT '',
  status         TEXT NOT NULL,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (session_id, call_id)
);

CREATE INDEX IF NOT EXISTS idx_oxpilot_tool_calls_message
  ON oxpilot_tool_calls (session_id, message_id, call_index);

CREATE TABLE IF NOT EXISTS oxpilot_queued_messages (
  id            TEXT PRIMARY KEY,
  session_id    TEXT NOT NULL REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  queue_seq     BIGINT NOT NULL,
  content       TEXT NOT NULL DEFAULT '',
  client_msg_id TEXT NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (session_id, queue_seq),
  UNIQUE (session_id, client_msg_id)
);

CREATE INDEX IF NOT EXISTS idx_oxpilot_queued_messages_session
  ON oxpilot_queued_messages (session_id);

CREATE TABLE IF NOT EXISTS oxpilot_proposals (
  id         TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  title      TEXT NOT NULL,
  base       TEXT NOT NULL,
  diff_body  TEXT NOT NULL DEFAULT '',
  status     TEXT NOT NULL DEFAULT 'pending',
  pr_ref     TEXT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS oxpilot_usage_counters (
  id         TEXT PRIMARY KEY,
  scope      TEXT NOT NULL,
  scope_key  TEXT NOT NULL,
  window     TEXT NOT NULL,
  requests   BIGINT NOT NULL DEFAULT 0,
  tokens     BIGINT NOT NULL DEFAULT 0,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (scope, scope_key, window)
);
