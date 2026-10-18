-- logical: 0064_action_reusable_calls — reusable-workflow caller/callee
-- linkage (actions parity P13).
--
-- `caller_job_id` points at the `uses:` job that expanded a callee row;
-- '' marks top-level jobs (NOT NULL keeps the dedupe tuple total — NULL in
-- a UNIQUE column never collides and would silently drop top-level dedupe).
-- The unique index gains the column so callee rows from distinct callers
-- sharing (job_key, matrix_key) no longer collide.

ALTER TABLE action_jobs
  ADD COLUMN caller_job_id VARCHAR(64) NOT NULL DEFAULT '';

ALTER TABLE action_jobs
  DROP INDEX uq_action_jobs_run_job_matrix,
  ADD UNIQUE KEY uq_action_jobs_run_job_matrix_caller
    (run_id, job_key, matrix_key, caller_job_id);

-- call-close pass probes children by caller inside one run.
CREATE INDEX idx_action_jobs_caller
  ON action_jobs(run_id, caller_job_id);
