-- logical: 0070_action_annotations — structured annotations extracted from
-- runner-emitted `##[ann]` log markers (GitHub's `::error|warning|notice::`
-- workflow commands surfaced as run/job annotations). Field names follow
-- GitHub's check-run annotation shape: path/start_line/end_line/
-- start_column/end_column/title/message/annotation_level.

CREATE TABLE action_annotations (
  id               TEXT PRIMARY KEY,
  run_id           TEXT NOT NULL REFERENCES action_runs(id) ON DELETE CASCADE,
  job_id           TEXT NOT NULL REFERENCES action_jobs(id) ON DELETE CASCADE,
  annotation_level TEXT NOT NULL,            -- error | warning | notice
  message          TEXT NOT NULL,
  path             TEXT NULL,
  start_line       INTEGER NULL,
  end_line         INTEGER NULL,
  start_column     INTEGER NULL,
  end_column       INTEGER NULL,
  title            TEXT NULL,
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_action_annotations_job ON action_annotations(job_id);
CREATE INDEX idx_action_annotations_run ON action_annotations(run_id);
