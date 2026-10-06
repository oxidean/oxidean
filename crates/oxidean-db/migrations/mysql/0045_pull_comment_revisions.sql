-- logical: 0045_pull_comment_revisions — edit history for pull comments
-- (mirrors comment_revisions, which FKs to issue_comments and cannot be
-- reused). Deleting a pull comment cascades its revisions.

CREATE TABLE IF NOT EXISTS pull_comment_revisions (
  id         CHAR(36)      PRIMARY KEY,
  comment_id CHAR(36)      NOT NULL,
  editor_id  CHAR(36)      NOT NULL,
  body       TEXT          NOT NULL,
  created_at TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_pull_comment_revisions_comment FOREIGN KEY (comment_id) REFERENCES pull_comments(id) ON DELETE CASCADE,
  CONSTRAINT fk_pull_comment_revisions_editor FOREIGN KEY (editor_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_pull_comment_revisions_comment_id ON pull_comment_revisions(comment_id);
