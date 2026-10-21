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

ALTER TABLE repositories ADD COLUMN discussions_enabled BOOLEAN NOT NULL DEFAULT true;

CREATE TABLE IF NOT EXISTS discussion_categories (
  id            TEXT PRIMARY KEY,
  repo_id       TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  slug          TEXT NOT NULL,
  emoji         TEXT NOT NULL DEFAULT '',
  description   TEXT NOT NULL DEFAULT '',
  is_answerable BOOLEAN NOT NULL DEFAULT false,
  position      BIGINT NOT NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (repo_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_discussion_categories_repo
  ON discussion_categories (repo_id, position);

CREATE TABLE IF NOT EXISTS discussions (
  id                TEXT PRIMARY KEY,
  repo_id           TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  number            BIGINT NOT NULL,
  title             TEXT NOT NULL,
  body              TEXT NOT NULL DEFAULT '',
  category_id       TEXT NULL REFERENCES discussion_categories(id) ON DELETE SET NULL,
  author_id         TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  locked            BOOLEAN NOT NULL DEFAULT false,
  pinned            BOOLEAN NOT NULL DEFAULT false,
  answer_comment_id TEXT NULL,
  answered_at       TIMESTAMPTZ NULL,
  answered_by       TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (repo_id, number)
);

CREATE INDEX IF NOT EXISTS idx_discussions_repo
  ON discussions (repo_id, pinned DESC, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_discussions_category
  ON discussions (category_id);

CREATE TABLE IF NOT EXISTS discussion_comments (
  id            TEXT PRIMARY KEY,
  discussion_id TEXT NOT NULL REFERENCES discussions(id) ON DELETE CASCADE,
  parent_id     TEXT NULL REFERENCES discussion_comments(id) ON DELETE CASCADE,
  author_id     TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  body          TEXT NOT NULL DEFAULT '',
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_discussion_comments_discussion
  ON discussion_comments (discussion_id, created_at);
CREATE INDEX IF NOT EXISTS idx_discussion_comments_parent
  ON discussion_comments (parent_id);

CREATE TABLE IF NOT EXISTS discussion_labels (
  discussion_id TEXT NOT NULL REFERENCES discussions(id) ON DELETE CASCADE,
  label_id      TEXT NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (discussion_id, label_id)
);

CREATE INDEX IF NOT EXISTS idx_discussion_labels_label
  ON discussion_labels (label_id);
