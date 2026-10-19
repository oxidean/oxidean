-- logical: 0070_action_annotations — structured annotations extracted from
-- runner-emitted `##[ann]` log markers (GitHub's `::error|warning|notice::`
-- workflow commands surfaced as run/job annotations). Field names follow
-- GitHub's check-run annotation shape: path/start_line/end_line/
-- start_column/end_column/title/message/annotation_level.

CREATE TABLE action_annotations (
  id               VARCHAR(64) PRIMARY KEY,
  run_id           CHAR(36) NOT NULL,
  job_id           CHAR(36) NOT NULL,
  annotation_level VARCHAR(16) NOT NULL,     -- error | warning | notice
  message          TEXT NOT NULL,
  path             TEXT NULL,
  start_line       INT NULL,
  end_line         INT NULL,
  start_column     INT NULL,
  end_column       INT NULL,
  title            TEXT NULL,
  created_at       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_action_annotations_run FOREIGN KEY (run_id) REFERENCES action_runs(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_annotations_job FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_annotations_job ON action_annotations(job_id);
CREATE INDEX idx_action_annotations_run ON action_annotations(run_id);
