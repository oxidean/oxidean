-- logical: 0033_deploy_keys — per-repo SSH deploy keys (GIT-23)
-- Deploy keys are transport-only credentials: they authorize git-upload-pack /
-- git-receive-pack for one repository and never act as an account identity.
-- Same public key may be attached to multiple repos but only once per repo
-- (UNIQUE KEY(repo_id, fingerprint)); a fingerprint already registered as an
-- account key (ssh_public_keys) is rejected at the RPC layer.
-- MySQL: TEXT cannot carry DEFAULT — use VARCHAR/CHAR where a default is required.
CREATE TABLE IF NOT EXISTS deploy_keys (
  id             CHAR(36)     PRIMARY KEY,
  repo_id        CHAR(36)     NOT NULL,
  title          VARCHAR(200) NOT NULL,
  public_key     TEXT         NOT NULL,
  fingerprint    VARCHAR(128) NOT NULL,
  key_type       VARCHAR(64)  NOT NULL,
  can_write      TINYINT(1)   NOT NULL DEFAULT 0,
  last_used_at   TIMESTAMP    NULL,
  last_used_ip   VARCHAR(64)  NULL,
  created_by     CHAR(36)     NOT NULL,
  created_at     TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY uk_deploy_keys_repo_fingerprint (repo_id, fingerprint),
  CONSTRAINT fk_deploy_keys_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_deploy_keys_created_by FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_deploy_keys_fingerprint ON deploy_keys(fingerprint);
CREATE INDEX idx_deploy_keys_repo_id ON deploy_keys(repo_id);
