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
  id             CHAR(36) PRIMARY KEY,
  user_id        CHAR(36) NOT NULL,
  code_hash      CHAR(64) NOT NULL,
  target_name    VARCHAR(255) NOT NULL,
  requested_caps MEDIUMTEXT NOT NULL,
  state          VARCHAR(32) NOT NULL DEFAULT 'pending',
  expires_at     DATETIME NOT NULL,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uq_oxpilot_pairings_code_hash (code_hash)
);

CREATE INDEX idx_oxpilot_pairings_user
  ON oxpilot_pairings (user_id);

CREATE TABLE IF NOT EXISTS oxpilot_targets (
  id              CHAR(36) PRIMARY KEY,
  user_id         CHAR(36) NOT NULL,
  pairing_id      CHAR(36) NOT NULL,
  name            VARCHAR(255) NOT NULL,
  caps            MEDIUMTEXT NOT NULL,
  status          VARCHAR(32) NOT NULL DEFAULT 'pending_consent',
  verifier_hash   CHAR(64) NULL,
  last_seen_at    DATETIME NULL,
  created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  disconnected_at DATETIME NULL,
  revoked_at      DATETIME NULL,
  UNIQUE KEY uq_oxpilot_targets_verifier (verifier_hash)
);

CREATE INDEX idx_oxpilot_targets_user
  ON oxpilot_targets (user_id);

CREATE TABLE IF NOT EXISTS oxpilot_grants (
  id             CHAR(36) PRIMARY KEY,
  target_id      CHAR(36) NOT NULL,
  user_id        CHAR(36) NOT NULL,
  session_id     CHAR(36) NULL,
  kind           VARCHAR(32) NOT NULL,
  action_kind    VARCHAR(32) NULL,
  action_args    MEDIUMTEXT NULL,
  action_summary MEDIUMTEXT NULL,
  state          VARCHAR(32) NOT NULL DEFAULT 'pending',
  expires_at     DATETIME NOT NULL,
  resolved_at    DATETIME NULL,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_oxpilot_grants_target FOREIGN KEY (target_id) REFERENCES oxpilot_targets(id) ON DELETE CASCADE,
  CONSTRAINT fk_oxpilot_grants_session FOREIGN KEY (session_id) REFERENCES oxpilot_sessions(id) ON DELETE CASCADE
);

CREATE INDEX idx_oxpilot_grants_target_state
  ON oxpilot_grants (target_id, state);
CREATE INDEX idx_oxpilot_grants_session
  ON oxpilot_grants (session_id);
CREATE INDEX idx_oxpilot_grants_user_state_created
  ON oxpilot_grants (user_id, state, created_at);

CREATE TABLE IF NOT EXISTS oxpilot_actions (
  id             CHAR(36) PRIMARY KEY,
  target_id      CHAR(36) NOT NULL,
  user_id        CHAR(36) NOT NULL,
  session_id     CHAR(36) NOT NULL,
  grant_id       CHAR(36) NOT NULL,
  kind           VARCHAR(32) NOT NULL,
  args           MEDIUMTEXT NOT NULL,
  args_summary   MEDIUMTEXT NOT NULL,
  state          VARCHAR(32) NOT NULL DEFAULT 'queued',
  result_summary MEDIUMTEXT NULL,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  claimed_at     DATETIME NULL,
  finished_at    DATETIME NULL,
  CONSTRAINT fk_oxpilot_actions_target FOREIGN KEY (target_id) REFERENCES oxpilot_targets(id) ON DELETE CASCADE,
  CONSTRAINT fk_oxpilot_actions_session FOREIGN KEY (session_id) REFERENCES oxpilot_sessions(id) ON DELETE CASCADE,
  CONSTRAINT fk_oxpilot_actions_grant FOREIGN KEY (grant_id) REFERENCES oxpilot_grants(id) ON DELETE CASCADE
);

-- FIFO dequeue index: state-conditional scan ordered by (created_at, id).
CREATE INDEX idx_oxpilot_actions_dequeue
  ON oxpilot_actions (target_id, state, created_at, id);
CREATE INDEX idx_oxpilot_actions_session
  ON oxpilot_actions (session_id);
CREATE INDEX idx_oxpilot_actions_user
  ON oxpilot_actions (user_id);
