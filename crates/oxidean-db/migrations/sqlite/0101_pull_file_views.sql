-- logical: 0101_pull_file_views — per-user "file viewed" marks on pull
-- requests (WI-0001 pr-new-ui). One row per (pull, user, path); head_sha
-- records which head the mark was taken against so the UI can invalidate
-- after a push if it wants to.

CREATE TABLE IF NOT EXISTS pull_file_views (
  pull_id   TEXT NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
  user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  path      TEXT NOT NULL,
  head_sha  TEXT NOT NULL,
  viewed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (pull_id, user_id, path)
);
