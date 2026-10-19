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
-- `seq` comes from rowid: the implicit rowid is the monotonic insertion
-- order the PG/MySQL `seq` columns mirror.

CREATE TABLE IF NOT EXISTS repo_pages (
  repo_id        TEXT PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  build_type     TEXT NOT NULL DEFAULT 'legacy'
    CHECK (build_type IN ('legacy', 'workflow')),
  source_branch  TEXT NOT NULL DEFAULT '',
  source_path    TEXT NOT NULL DEFAULT '/',
  cname          TEXT NOT NULL DEFAULT '',
  https_enforced INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE TABLE IF NOT EXISTS pages_builds (
  id            TEXT PRIMARY KEY,
  repo_id       TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  status        TEXT NOT NULL
    CHECK (status IN ('building', 'built', 'errored')),
  build_type    TEXT NOT NULL DEFAULT 'legacy'
    CHECK (build_type IN ('legacy', 'workflow')),
  commit_sha    TEXT NOT NULL DEFAULT '',
  branch        TEXT NOT NULL DEFAULT '',
  path          TEXT NOT NULL DEFAULT '',
  url           TEXT NOT NULL DEFAULT '',
  error_message TEXT NOT NULL DEFAULT '',
  duration_ms   INTEGER NOT NULL DEFAULT 0,
  pusher_id     TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  run_id        TEXT NULL,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

CREATE INDEX IF NOT EXISTS idx_pages_builds_repo_created
  ON pages_builds (repo_id, created_at DESC);
