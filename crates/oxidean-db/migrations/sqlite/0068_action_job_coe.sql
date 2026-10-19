-- logical: 0065_action_job_coe — resolved `continue-on-error` per job leg.
--
-- `jobs.<id>.continue-on-error` suppresses a leg's failure for run-rollup
-- and `fail-fast` purposes (the leg itself still reports `failure`). The
-- resolved bool is stamped at emit — expression/matrix evaluation happens
-- once so the rollup SQL needs no payload access.

ALTER TABLE action_jobs ADD COLUMN continue_on_error INTEGER NOT NULL DEFAULT 0;
