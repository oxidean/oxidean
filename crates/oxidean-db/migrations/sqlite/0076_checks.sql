-- Checks domain (GitHub Checks API parity):
--  * check_suites group check_runs per (repo, sha, app); Actions runs get a
--    suite each (`run_id` linkage), API-created runs share the `api` suite
--    per commit.
--  * check_runs carry GitHub's status/conclusion vocabulary plus optional
--    job/run linkage for Actions-projected checks.
--  * check_run_annotations backs `check.runs.annotations.list` output.
-- `seq` columns carry insertion order — the monotonic ORDER BY tiebreak on
-- created_at used by every newest-first query (see 0072).

CREATE TABLE check_suites (
  id              TEXT PRIMARY KEY,
  repository_id   TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  head_sha        TEXT NOT NULL,
  head_branch     TEXT NULL,
  app             TEXT NOT NULL DEFAULT 'api',
  run_id          TEXT NULL REFERENCES action_runs(id) ON DELETE SET NULL,
  status          TEXT NOT NULL DEFAULT 'queued',
  conclusion      TEXT NULL,
  url             TEXT NULL,
  creator_id      TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
CREATE INDEX idx_check_suites_repo_sha ON check_suites(repository_id, head_sha);
CREATE INDEX idx_check_suites_run ON check_suites(run_id);

CREATE TABLE check_runs (
  id              TEXT PRIMARY KEY,
  repository_id   TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  check_suite_id  TEXT NOT NULL REFERENCES check_suites(id) ON DELETE CASCADE,
  name            TEXT NOT NULL,
  head_sha        TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'queued',
  conclusion      TEXT NULL,
  details_url     TEXT NULL,
  external_id     TEXT NULL,
  output_title    TEXT NULL,
  output_summary  TEXT NULL,
  output_text     TEXT NULL,
  job_id          TEXT NULL REFERENCES action_jobs(id) ON DELETE SET NULL,
  run_id          TEXT NULL REFERENCES action_runs(id) ON DELETE SET NULL,
  creator_id      TEXT NULL REFERENCES users(id) ON DELETE SET NULL,
  started_at      TEXT NULL,
  completed_at    TEXT NULL,
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
CREATE INDEX idx_check_runs_suite ON check_runs(check_suite_id);
CREATE INDEX idx_check_runs_repo_sha ON check_runs(repository_id, head_sha);
CREATE INDEX idx_check_runs_job ON check_runs(job_id);
CREATE INDEX idx_check_runs_run ON check_runs(run_id);

CREATE TABLE check_run_annotations (
  id               TEXT PRIMARY KEY,
  check_run_id     TEXT NOT NULL REFERENCES check_runs(id) ON DELETE CASCADE,
  repository_id    TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  path             TEXT NOT NULL,
  start_line       INTEGER NOT NULL,
  end_line         INTEGER NOT NULL,
  start_column     INTEGER NULL,
  end_column       INTEGER NULL,
  annotation_level TEXT NOT NULL,
  message          TEXT NOT NULL,
  title            TEXT NULL,
  raw_details      TEXT NULL,
  created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
CREATE INDEX idx_check_run_annotations_run ON check_run_annotations(check_run_id);
