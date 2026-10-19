-- logical: 0065_action_job_coe — resolved `continue-on-error` per job leg.

ALTER TABLE action_jobs
  ADD COLUMN continue_on_error TINYINT(1) NOT NULL DEFAULT 0;
