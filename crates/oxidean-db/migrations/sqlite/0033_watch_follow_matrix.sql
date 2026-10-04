-- logical: 0033_watch_follow_matrix — per-repo watch notification levels
-- and asymmetric user follows (DEBT-06).

-- Watch level matrix: 'all' = notified on repo activity (existing watchers
-- backfill here — watching was already the subscribe intent), 'participating'
-- = only when participating or @-mentioned, 'ignore' = never (suppresses
-- participation and mention rows too).
ALTER TABLE repository_watches ADD COLUMN level TEXT NOT NULL DEFAULT 'all'
  CHECK (level IN ('all', 'participating', 'ignore'));

-- Asymmetric user follow graph; no denormalized counters (counts computed on read).
CREATE TABLE IF NOT EXISTS user_follows (
  follower_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  followed_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (follower_id, followed_id),
  CHECK (follower_id <> followed_id)
);

CREATE INDEX IF NOT EXISTS idx_user_follows_followed
  ON user_follows(followed_id);
CREATE INDEX IF NOT EXISTS idx_user_follows_follower_created
  ON user_follows(follower_id, created_at DESC);
