-- logical: 0096_user_pinned_repos — pinned repositories on user profiles
-- (COL-04 profile half, GitHub parity). `position` is the 0-based display
-- order, unique per user; ON DELETE CASCADE drops pins with user or repo.

CREATE TABLE IF NOT EXISTS user_pinned_repos (
  user_id       TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  repository_id TEXT        NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  position      INTEGER     NOT NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, repository_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_user_pinned_repos_user_position
  ON user_pinned_repos(user_id, position);
