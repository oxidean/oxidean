-- logical: 0031_repository_invites — per-repo email invites (read|write|admin)

CREATE TABLE IF NOT EXISTS repository_invites (
  id            CHAR(36)     PRIMARY KEY,
  repository_id CHAR(36)     NOT NULL,
  email         VARCHAR(320) NOT NULL,
  permission    VARCHAR(16)  NOT NULL,
  token_hash    CHAR(64)     NOT NULL,
  expires_at    TIMESTAMP    NOT NULL,
  invited_by    CHAR(36)     NOT NULL,
  created_at    TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
  accepted_at   TIMESTAMP    NULL,
  revoked_at    TIMESTAMP    NULL,
  UNIQUE KEY uk_repository_invites_token_hash (token_hash),
  CONSTRAINT fk_repo_invites_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_repo_invites_invited_by FOREIGN KEY (invited_by) REFERENCES users(id) ON DELETE CASCADE,
  CONSTRAINT repository_invites_permission_check
    CHECK (permission IN ('read', 'write', 'admin'))
) ENGINE=InnoDB;

CREATE INDEX idx_repository_invites_repository_id ON repository_invites(repository_id);
CREATE INDEX idx_repository_invites_email ON repository_invites(email);
