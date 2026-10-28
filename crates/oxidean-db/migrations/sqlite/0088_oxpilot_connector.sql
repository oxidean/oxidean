-- logical: 0088_oxpilot_connector — the U6 connector-gateway tables:
-- pairing codes, connector targets, consent grants, and the durable
-- action queue.
--
-- `oxpilot_pairings` is the single-use mint surface: `code_hash` is
-- SHA-256 hex of the shown-once pairing code (the plaintext never lands
-- in storage, logs, or audit); `state` drives the lazy-expiry
-- pending→claimed|expired transition CASed at redeem.
--
-- `oxpilot_targets` is the connector-owned machine — `verifier_hash`
-- (SHA-256 hex) is tombstoned (NULLed) on disconnect but survives revoke
-- so a revoked connector's heartbeat can still answer `revoked:true`.
-- `caps`/`requested_caps` are the stored JSON capability arrays.
--
-- `oxpilot_grants` carries BOTH connect grants (`session_id IS NULL` —
-- survives session delete, BR6.6) and session-scoped action grants
-- (`session_id` set — cascades with the session). `action_args`/
-- `action_summary` arrive already secrets-redacted and bounded; `kind`
-- is `connect | action | action_once | action_session` (the pending
-- action grant stamps its flavor at respond time).
--
-- `oxpilot_actions` is the at-most-once work queue: `args` and
-- `args_summary`/`result_summary` are pre-redacted bounded copies (raw
-- connector output never persists). `grant_id` points at the covering
-- allowed grant so dequeue can re-check liveness; all session-scoped
-- rows cascade with `oxpilot_sessions` while durable audit rows survive.

CREATE TABLE IF NOT EXISTS oxpilot_pairings (
  id             TEXT PRIMARY KEY,
  user_id        TEXT NOT NULL,
  code_hash      TEXT NOT NULL,
  target_name    TEXT NOT NULL,
  requested_caps TEXT NOT NULL,
  state          TEXT NOT NULL DEFAULT 'pending',
  expires_at     TEXT NOT NULL,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (code_hash)
);

CREATE INDEX IF NOT EXISTS idx_oxpilot_pairings_user
  ON oxpilot_pairings (user_id);

CREATE TABLE IF NOT EXISTS oxpilot_targets (
  id              TEXT PRIMARY KEY,
  user_id         TEXT NOT NULL,
  pairing_id      TEXT NOT NULL,
  name            TEXT NOT NULL,
  caps            TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'pending_consent',
  verifier_hash   TEXT NULL,
  last_seen_at    TEXT NULL,
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  disconnected_at TEXT NULL,
  revoked_at      TEXT NULL
);

-- One live verifier per target; tombstoned rows fall out of the index.
CREATE UNIQUE INDEX IF NOT EXISTS uq_oxpilot_targets_verifier
  ON oxpilot_targets (verifier_hash) WHERE verifier_hash IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_oxpilot_targets_user
  ON oxpilot_targets (user_id);

CREATE TABLE IF NOT EXISTS oxpilot_grants (
  id             TEXT PRIMARY KEY,
  target_id      TEXT NOT NULL REFERENCES oxpilot_targets(id) ON DELETE CASCADE,
  user_id        TEXT NOT NULL,
  session_id     TEXT NULL REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  kind           TEXT NOT NULL,
  action_kind    TEXT NULL,
  action_args    TEXT NULL,
  action_summary TEXT NULL,
  state          TEXT NOT NULL DEFAULT 'pending',
  expires_at     TEXT NOT NULL,
  resolved_at    TEXT NULL,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_oxpilot_grants_target_state
  ON oxpilot_grants (target_id, state);
CREATE INDEX IF NOT EXISTS idx_oxpilot_grants_session
  ON oxpilot_grants (session_id);
CREATE INDEX IF NOT EXISTS idx_oxpilot_grants_user_state_created
  ON oxpilot_grants (user_id, state, created_at);

CREATE TABLE IF NOT EXISTS oxpilot_actions (
  id             TEXT PRIMARY KEY,
  target_id      TEXT NOT NULL REFERENCES oxpilot_targets(id) ON DELETE CASCADE,
  user_id        TEXT NOT NULL,
  session_id     TEXT NOT NULL REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  grant_id       TEXT NOT NULL REFERENCES oxpilot_grants(id) ON DELETE CASCADE,
  kind           TEXT NOT NULL,
  args           TEXT NOT NULL,
  args_summary   TEXT NOT NULL DEFAULT '',
  state          TEXT NOT NULL DEFAULT 'queued',
  result_summary TEXT NULL,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  claimed_at     TEXT NULL,
  finished_at    TEXT NULL
);

-- FIFO dequeue index: state-conditional scan ordered by (created_at, id).
CREATE INDEX IF NOT EXISTS idx_oxpilot_actions_dequeue
  ON oxpilot_actions (target_id, state, created_at, id);
CREATE INDEX IF NOT EXISTS idx_oxpilot_actions_session
  ON oxpilot_actions (session_id);
CREATE INDEX IF NOT EXISTS idx_oxpilot_actions_user
  ON oxpilot_actions (user_id);
