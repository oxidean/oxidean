-- logical: 0055_gitea_task_claims — task-id mapping + tasks_version counter
-- for the Gitea `runner.v1` compatibility surface (actions parity Phase 5).
-- act_runner identifies in-flight work by a never-reused numeric task_id and
-- polls with a monotonic tasks_version watermark.

CREATE TABLE action_task_claims (
  task_id    BIGSERIAL PRIMARY KEY,
  job_id     VARCHAR(64) NOT NULL REFERENCES action_jobs(id) ON DELETE CASCADE,
  runner_id  VARCHAR(64) NOT NULL REFERENCES action_runners(id) ON DELETE CASCADE,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_action_task_claims_job ON action_task_claims(job_id);

-- Single-row monotonic counters. `tasks` bumps on every claimable job
-- enqueue/requeue; FetchTask compares it against the runner's watermark.
CREATE TABLE action_counters (
  name  VARCHAR(64) PRIMARY KEY,
  value BIGINT NOT NULL DEFAULT 0
);
