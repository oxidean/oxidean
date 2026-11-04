-- logical: 0096_user_pinned_repos — pinned repositories on user profiles
-- (COL-04 profile half, GitHub parity). `position` is the 0-based display
-- order, unique per user; ON DELETE CASCADE drops pins with user or repo.

CREATE TABLE IF NOT EXISTS user_pinned_repos (
  user_id       CHAR(36) NOT NULL,
  repository_id CHAR(36) NOT NULL,
  position      INT      NOT NULL,
  created_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (user_id, repository_id),
  CONSTRAINT fk_upr_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
  CONSTRAINT fk_upr_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE UNIQUE INDEX idx_user_pinned_repos_user_position
  ON user_pinned_repos(user_id, position);
