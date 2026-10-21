-- logical: 0065_action_job_coe — resolved `continue-on-error` per job leg.

ALTER TABLE action_jobs ADD COLUMN continue_on_error BOOLEAN NOT NULL DEFAULT FALSE;
