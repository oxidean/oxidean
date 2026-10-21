-- logical: 0069_action_job_strategy — per-leg strategy metadata captured at
-- emit: `{"job_index","job_total","fail_fast","max_parallel"}`. The `strategy`
-- expression context reads this; index is expansion-order (matrix leg
-- ordinal), which cannot be re-derived at claim.

-- MySQL forbids literal defaults on TEXT columns (expression defaults only);
-- every writer binds strategy_json explicitly, so backfill instead.
ALTER TABLE action_jobs ADD COLUMN strategy_json TEXT NOT NULL;
UPDATE action_jobs SET strategy_json = '{}' WHERE strategy_json = '';
