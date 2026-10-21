-- logical: 0077_pages — GitHub Pages site config + build records.
--
-- `repo_pages` existence is the enabled flag (GitHub creates the site on
-- POST /pages; DELETE /pages unpublishes). `build_type`:
--   legacy   — serve `source_branch:source_path` (`/` or `/docs`),
--              rebuilt on every push to the branch.
--   workflow — an Actions deploy publishes a `github-pages` artifact via
--              the job-token-gated `pages.deploys.create` seam.
--
-- `pages_builds` rows are append-only history; `status` is `building` →
-- `built` | `errored`. Serving reads the live tree at
-- `OXIDEAN_PAGES_DIR/<repo_id>/` — the newest `built` row published it.
-- `page_build` fires once per row on completion (no activity types).

CREATE TABLE IF NOT EXISTS repo_pages (
  repo_id        CHAR(36) PRIMARY KEY,
  build_type     VARCHAR(16) NOT NULL DEFAULT 'legacy',
  source_branch  VARCHAR(255) NOT NULL DEFAULT '',
  source_path    VARCHAR(255) NOT NULL DEFAULT '/',
  cname          VARCHAR(255) NOT NULL DEFAULT '',
  https_enforced TINYINT(1) NOT NULL DEFAULT 1,
  created_at     TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at     TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT chk_repo_pages_build_type CHECK (build_type IN ('legacy', 'workflow')),
  CONSTRAINT fk_repo_pages_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pages_builds (
  id            CHAR(36) PRIMARY KEY,
  seq           BIGINT NOT NULL AUTO_INCREMENT UNIQUE,
  repo_id       CHAR(36) NOT NULL,
  status        VARCHAR(16) NOT NULL,
  build_type    VARCHAR(16) NOT NULL DEFAULT 'legacy',
  commit_sha    VARCHAR(64) NOT NULL DEFAULT '',
  branch        VARCHAR(255) NOT NULL DEFAULT '',
  path          VARCHAR(255) NOT NULL DEFAULT '',
  url           VARCHAR(512) NOT NULL DEFAULT '',
  error_message TEXT NOT NULL,
  duration_ms   BIGINT NOT NULL DEFAULT 0,
  pusher_id     CHAR(36) NULL,
  run_id        CHAR(36) NULL,
  created_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT chk_pages_builds_status CHECK (status IN ('building', 'built', 'errored')),
  CONSTRAINT chk_pages_builds_type CHECK (build_type IN ('legacy', 'workflow')),
  CONSTRAINT fk_pages_builds_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_pages_builds_pusher FOREIGN KEY (pusher_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX idx_pages_builds_repo_created
  ON pages_builds (repo_id, created_at DESC);
