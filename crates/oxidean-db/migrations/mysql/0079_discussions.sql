-- logical: 0076_discussions — repo-scoped discussion forums (GitHub
-- Discussions shape).
--
-- Discussions share `issue_counters` with issues/PRs (D-PR-02) — GitHub
-- numbers all three on one sequence, so `#N` never collides across kinds.
-- Categories carry `is_answerable` (Q&A); a Q&A discussion resolves an
-- answer by pointing `answer_comment_id` at a comment row (no FK — code
-- enforces comment↔discussion membership).
--
-- Comments thread one level deep (`parent_id` → top-level comment), matching
-- GitHub's reply model. Labels reuse the repo `labels` table through a join
-- row, like `issue_labels`/`pull_labels`.
--
-- `discussions_enabled` is the COL-13 unit flag — disabling never deletes
-- data; the `discussions.*` RPC surface and Discussions tab reject until
-- re-enabled.

ALTER TABLE repositories ADD COLUMN discussions_enabled TINYINT(1) NOT NULL DEFAULT 1;

CREATE TABLE IF NOT EXISTS discussion_categories (
  id            CHAR(36) PRIMARY KEY,
  repo_id       CHAR(36) NOT NULL,
  name          VARCHAR(255) NOT NULL,
  slug          VARCHAR(255) NOT NULL,
  emoji         VARCHAR(64) NOT NULL DEFAULT '',
  description   TEXT NOT NULL,
  is_answerable TINYINT(1) NOT NULL DEFAULT 0,
  position      BIGINT NOT NULL,
  created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  CONSTRAINT fk_discussion_categories_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  UNIQUE KEY uq_discussion_categories_repo_slug (repo_id, slug)
);

CREATE INDEX idx_discussion_categories_repo
  ON discussion_categories (repo_id, position);

CREATE TABLE IF NOT EXISTS discussions (
  id                CHAR(36) PRIMARY KEY,
  repo_id           CHAR(36) NOT NULL,
  number            BIGINT NOT NULL,
  title             VARCHAR(512) NOT NULL,
  body              MEDIUMTEXT NOT NULL,
  category_id       CHAR(36) NULL,
  author_id         CHAR(36) NULL,
  locked            TINYINT(1) NOT NULL DEFAULT 0,
  pinned            TINYINT(1) NOT NULL DEFAULT 0,
  answer_comment_id CHAR(36) NULL,
  answered_at       DATETIME NULL,
  answered_by       CHAR(36) NULL,
  created_at        DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at        DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  CONSTRAINT fk_discussions_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_discussions_category FOREIGN KEY (category_id) REFERENCES discussion_categories(id) ON DELETE SET NULL,
  CONSTRAINT fk_discussions_author FOREIGN KEY (author_id) REFERENCES users(id) ON DELETE SET NULL,
  CONSTRAINT fk_discussions_answered_by FOREIGN KEY (answered_by) REFERENCES users(id) ON DELETE SET NULL,
  UNIQUE KEY uq_discussions_repo_number (repo_id, number)
);

CREATE INDEX idx_discussions_repo
  ON discussions (repo_id, pinned DESC, updated_at);
CREATE INDEX idx_discussions_category
  ON discussions (category_id);

CREATE TABLE IF NOT EXISTS discussion_comments (
  id            CHAR(36) PRIMARY KEY,
  discussion_id CHAR(36) NOT NULL,
  parent_id     CHAR(36) NULL,
  author_id     CHAR(36) NULL,
  body          MEDIUMTEXT NOT NULL,
  created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  CONSTRAINT fk_discussion_comments_discussion FOREIGN KEY (discussion_id) REFERENCES discussions(id) ON DELETE CASCADE,
  CONSTRAINT fk_discussion_comments_parent FOREIGN KEY (parent_id) REFERENCES discussion_comments(id) ON DELETE CASCADE,
  CONSTRAINT fk_discussion_comments_author FOREIGN KEY (author_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX idx_discussion_comments_discussion
  ON discussion_comments (discussion_id, created_at);
CREATE INDEX idx_discussion_comments_parent
  ON discussion_comments (parent_id);

CREATE TABLE IF NOT EXISTS discussion_labels (
  discussion_id CHAR(36) NOT NULL,
  label_id      CHAR(36) NOT NULL,
  PRIMARY KEY (discussion_id, label_id),
  CONSTRAINT fk_discussion_labels_discussion FOREIGN KEY (discussion_id) REFERENCES discussions(id) ON DELETE CASCADE,
  CONSTRAINT fk_discussion_labels_label FOREIGN KEY (label_id) REFERENCES labels(id) ON DELETE CASCADE
);

CREATE INDEX idx_discussion_labels_label
  ON discussion_labels (label_id);
