-- logical: 0101_pull_file_views — per-user "file viewed" marks on pull
-- requests (WI-0001 pr-new-ui). One row per (pull, user, path); head_sha
-- records which head the mark was taken against so the UI can invalidate
-- after a push if it wants to.

CREATE TABLE IF NOT EXISTS pull_file_views (
  pull_id   CHAR(36)    NOT NULL,
  user_id   CHAR(36)    NOT NULL,
  path      VARCHAR(255) NOT NULL,
  head_sha  VARCHAR(64) NOT NULL,
  viewed_at TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (pull_id, user_id, path),
  CONSTRAINT fk_pfv_pull FOREIGN KEY (pull_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  CONSTRAINT fk_pfv_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;
