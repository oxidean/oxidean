-- logical: 0067_action_job_debug — per-attempt debug logging flag set by
-- "re-run with debug logging"; claim-time vars surface it to the runner.

ALTER TABLE action_jobs ADD COLUMN debug_logging BOOLEAN NOT NULL DEFAULT FALSE;
