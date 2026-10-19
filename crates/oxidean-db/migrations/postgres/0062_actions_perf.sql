-- logical: 0059_actions_perf — covering indexes for the hot read paths
-- (actions parity Phase 9).

-- list_runs_for_repo: WHERE repository_id = ? ORDER BY created_at DESC, id
-- DESC — the composite serves filter + order from one index, superseding the
-- 0021 single-column index.
CREATE INDEX IF NOT EXISTS idx_action_runs_repo_created
  ON action_runs(repository_id, created_at DESC, id DESC);
DROP INDEX IF EXISTS idx_action_runs_repository_id;

-- claim_queued_job_for_labels + the stale sweep: the queue scan filters
-- status='queued' and orders by created_at — (status, created_at) keeps the
-- candidate probe indexed instead of scanning the jobs table.
CREATE INDEX IF NOT EXISTS idx_action_jobs_status_created
  ON action_jobs(status, created_at);

-- The claim probe's NOT EXISTS leg joins action_job_labels on job_id first —
-- the 0053 index on (base) alone can't serve it.
CREATE INDEX IF NOT EXISTS idx_action_job_labels_job
  ON action_job_labels(job_id);
