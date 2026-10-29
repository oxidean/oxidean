-- logical: 0093_oxpilot_connector_user_fks — cascade connector rows
-- with their owner. `delete_user` issues a bare `DELETE FROM users`
-- and relies on ON DELETE CASCADE FKs everywhere; pairings/targets
-- lacked one, so a deleted user's verifier target stayed
-- authenticable (verifier_hash lookup returned the dead user_id).
-- SQLite cannot add a FK post-hoc — rebuild both tables (the 0044
-- pattern), copying only owner-live rows; children referencing a
-- dropped target are swept after the rebuild.
CREATE TABLE oxpilot_pairings_new (
  id             TEXT PRIMARY KEY,
  user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  code_hash      TEXT NOT NULL,
  target_name    TEXT NOT NULL,
  requested_caps TEXT NOT NULL,
  state          TEXT NOT NULL DEFAULT 'pending',
  expires_at     TEXT NOT NULL,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  UNIQUE (code_hash)
);

INSERT INTO oxpilot_pairings_new
  SELECT * FROM oxpilot_pairings
  WHERE user_id IN (SELECT id FROM users);

DROP TABLE oxpilot_pairings;
ALTER TABLE oxpilot_pairings_new RENAME TO oxpilot_pairings;

CREATE INDEX idx_oxpilot_pairings_user
  ON oxpilot_pairings (user_id);

CREATE TABLE oxpilot_targets_new (
  id              TEXT PRIMARY KEY,
  user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
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

INSERT INTO oxpilot_targets_new
  SELECT * FROM oxpilot_targets
  WHERE user_id IN (SELECT id FROM users);

DROP TABLE oxpilot_targets;
ALTER TABLE oxpilot_targets_new RENAME TO oxpilot_targets;

-- One live verifier per target; tombstoned rows fall out of the index.
CREATE UNIQUE INDEX uq_oxpilot_targets_verifier
  ON oxpilot_targets (verifier_hash) WHERE verifier_hash IS NOT NULL;
CREATE INDEX idx_oxpilot_targets_user
  ON oxpilot_targets (user_id);

-- Children of any dropped target would violate their own FKs on the
-- next write — sweep them now.
DELETE FROM oxpilot_actions WHERE target_id NOT IN (SELECT id FROM oxpilot_targets);
DELETE FROM oxpilot_grants  WHERE target_id NOT IN (SELECT id FROM oxpilot_targets);
