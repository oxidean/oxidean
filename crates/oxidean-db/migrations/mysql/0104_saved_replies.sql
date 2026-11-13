-- logical: 0104_saved_replies — per-user saved comment replies (GitHub
-- "saved replies"). One row per (user, reply); `name` is the picker label,
-- `body` the markdown inserted into a composer. User-scoped — no repo/org
-- binding in v1.

CREATE TABLE IF NOT EXISTS saved_replies (
  id         CHAR(36)    PRIMARY KEY,
  user_id    CHAR(36)    NOT NULL,
  name       VARCHAR(255) NOT NULL,
  body       TEXT        NOT NULL,
  created_at DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  CONSTRAINT fk_saved_replies_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_saved_replies_user ON saved_replies(user_id, updated_at);
