-- logical: 0033_watch_follow_matrix — per-repo watch notification levels
-- and asymmetric user follows (DEBT-06).

-- Watch level matrix: 'all' = notified on repo activity (existing watchers
-- backfill here — watching was already the subscribe intent), 'participating'
-- = only when participating or @-mentioned, 'ignore' = never (suppresses
-- participation and mention rows too).
ALTER TABLE repository_watches ADD COLUMN level VARCHAR(32) NOT NULL DEFAULT 'all';
ALTER TABLE repository_watches ADD CONSTRAINT repository_watches_level_check
  CHECK (level IN ('all', 'participating', 'ignore'));

-- Asymmetric user follow graph; no denormalized counters (counts computed on read).
CREATE TABLE IF NOT EXISTS user_follows (
  follower_id CHAR(36) NOT NULL,
  followed_id CHAR(36) NOT NULL,
  created_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  PRIMARY KEY (follower_id, followed_id),
  CONSTRAINT fk_user_follows_follower
    FOREIGN KEY (follower_id) REFERENCES users(id) ON DELETE CASCADE,
  CONSTRAINT fk_user_follows_followed
    FOREIGN KEY (followed_id) REFERENCES users(id) ON DELETE CASCADE,
  CONSTRAINT user_follows_no_self CHECK (follower_id <> followed_id)
) ENGINE=InnoDB;

CREATE INDEX idx_user_follows_followed
  ON user_follows(followed_id);
CREATE INDEX idx_user_follows_follower_created
  ON user_follows(follower_id, created_at DESC);
