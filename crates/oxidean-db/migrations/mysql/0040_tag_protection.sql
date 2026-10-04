-- logical: 0033_tag_protection — protected tag rulesets (GIT-21)

CREATE TABLE IF NOT EXISTS tag_protection_rules (
  id             CHAR(36)     PRIMARY KEY,
  repo_id        CHAR(36)     NOT NULL,
  pattern        VARCHAR(255) NOT NULL,
  allow_create   TINYINT(1)   NOT NULL DEFAULT 0,
  allow_update   TINYINT(1)   NOT NULL DEFAULT 0,
  allow_delete   TINYINT(1)   NOT NULL DEFAULT 0,
  enforce_admins TINYINT(1)   NOT NULL DEFAULT 0,
  created_at     TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at     TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_tag_protection_rules_repo
    FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_tag_protection_rules_repo ON tag_protection_rules(repo_id);
