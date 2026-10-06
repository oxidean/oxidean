-- logical: 0045_pull_comment_revisions — edit history for pull comments
-- (mirrors comment_revisions, which FKs to issue_comments and cannot be
-- reused). Deleting a pull comment cascades its revisions.

CREATE TABLE IF NOT EXISTS pull_comment_revisions (
  id         TEXT PRIMARY KEY,
  comment_id TEXT NOT NULL REFERENCES pull_comments(id) ON DELETE CASCADE,
  editor_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  body       TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_pull_comment_revisions_comment_id ON pull_comment_revisions(comment_id);
