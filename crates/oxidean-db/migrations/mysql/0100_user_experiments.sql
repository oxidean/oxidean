-- logical: 0100_user_experiments — per-user experiment flags (WI-0001).
-- Row present = flag enabled for that user; keys are validated against the
-- oxidean-core EXPERIMENTS registry, never trusted from the wire. Flag
-- retirement deletes the registry entry + gate sites; stale rows are inert.

CREATE TABLE IF NOT EXISTS user_experiments (
  user_id    CHAR(36)   NOT NULL,
  flag_key   VARCHAR(64) NOT NULL,
  enabled_at TIMESTAMP  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (user_id, flag_key),
  CONSTRAINT fk_uex_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;
