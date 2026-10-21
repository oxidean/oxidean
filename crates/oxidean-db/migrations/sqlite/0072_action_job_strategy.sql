-- logical: 0069_action_job_strategy — per-leg strategy metadata captured at
-- emit: `{"job_index","job_total","fail_fast","max_parallel"}`. The `strategy`
-- expression context reads this; index is expansion-order (matrix leg
-- ordinal), which cannot be re-derived at claim.

ALTER TABLE action_jobs ADD COLUMN strategy_json TEXT NOT NULL DEFAULT '{}';
