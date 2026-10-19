-- Checks domain (GitHub Checks API parity):
--  * check_suites group check_runs per (repo, sha, app); Actions runs get a
--    suite each (`run_id` linkage), API-created runs share the `api` suite
--    per commit.
--  * check_runs carry GitHub's status/conclusion vocabulary plus optional
--    job/run linkage for Actions-projected checks.
--  * check_run_annotations backs `check.runs.annotations.list` output.
-- `seq` auto_increment columns carry insertion order — the monotonic ORDER
-- BY tiebreak on created_at used by every newest-first query (see 0072).

-- FK columns referencing CHAR(36) parents must be CHAR(36) too: MySQL's InnoDB
-- FK probe compares stored widths, so a VARCHAR child only matches parent rows
-- whose value is exactly 36 chars (e.g. any non-UUID repo id FK-fails).
CREATE TABLE check_suites (
  id              VARCHAR(64) PRIMARY KEY,
  seq             BIGINT NOT NULL AUTO_INCREMENT UNIQUE,
  repository_id   CHAR(36) NOT NULL,
  head_sha        VARCHAR(64) NOT NULL,
  head_branch     VARCHAR(255) NULL,
  app             VARCHAR(64) NOT NULL DEFAULT 'api',
  run_id          CHAR(36) NULL,
  status          VARCHAR(32) NOT NULL DEFAULT 'queued',
  conclusion      VARCHAR(32) NULL,
  url             TEXT NULL,
  creator_id      CHAR(36) NULL,
  created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_check_suites_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_check_suites_run FOREIGN KEY (run_id) REFERENCES action_runs(id) ON DELETE SET NULL,
  CONSTRAINT fk_check_suites_creator FOREIGN KEY (creator_id) REFERENCES users(id) ON DELETE SET NULL
);
CREATE INDEX idx_check_suites_repo_sha ON check_suites(repository_id, head_sha);
CREATE INDEX idx_check_suites_run ON check_suites(run_id);

CREATE TABLE check_runs (
  id              VARCHAR(64) PRIMARY KEY,
  seq             BIGINT NOT NULL AUTO_INCREMENT UNIQUE,
  repository_id   CHAR(36) NOT NULL,
  check_suite_id  VARCHAR(64) NOT NULL,
  name            VARCHAR(255) NOT NULL,
  head_sha        VARCHAR(64) NOT NULL,
  status          VARCHAR(32) NOT NULL DEFAULT 'queued',
  conclusion      VARCHAR(32) NULL,
  details_url     TEXT NULL,
  external_id     VARCHAR(128) NULL,
  output_title    TEXT NULL,
  output_summary  TEXT NULL,
  output_text     MEDIUMTEXT NULL,
  job_id          CHAR(36) NULL,
  run_id          CHAR(36) NULL,
  creator_id      CHAR(36) NULL,
  started_at      VARCHAR(64) NULL,
  completed_at    VARCHAR(64) NULL,
  created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_check_runs_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_check_runs_suite FOREIGN KEY (check_suite_id) REFERENCES check_suites(id) ON DELETE CASCADE,
  CONSTRAINT fk_check_runs_job FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE SET NULL,
  CONSTRAINT fk_check_runs_run FOREIGN KEY (run_id) REFERENCES action_runs(id) ON DELETE SET NULL,
  CONSTRAINT fk_check_runs_creator FOREIGN KEY (creator_id) REFERENCES users(id) ON DELETE SET NULL
);
CREATE INDEX idx_check_runs_suite ON check_runs(check_suite_id);
CREATE INDEX idx_check_runs_repo_sha ON check_runs(repository_id, head_sha);
CREATE INDEX idx_check_runs_job ON check_runs(job_id);
CREATE INDEX idx_check_runs_run ON check_runs(run_id);

CREATE TABLE check_run_annotations (
  id               VARCHAR(64) PRIMARY KEY,
  seq              BIGINT NOT NULL AUTO_INCREMENT UNIQUE,
  check_run_id     VARCHAR(64) NOT NULL,
  repository_id    CHAR(36) NOT NULL,
  path             TEXT NOT NULL,
  start_line       BIGINT NOT NULL,
  end_line         BIGINT NOT NULL,
  start_column     BIGINT NULL,
  end_column       BIGINT NULL,
  annotation_level VARCHAR(32) NOT NULL,
  message          TEXT NOT NULL,
  title            TEXT NULL,
  raw_details      MEDIUMTEXT NULL,
  created_at       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_check_run_annotations_run FOREIGN KEY (check_run_id) REFERENCES check_runs(id) ON DELETE CASCADE,
  CONSTRAINT fk_check_run_annotations_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
);
CREATE INDEX idx_check_run_annotations_run ON check_run_annotations(check_run_id);
