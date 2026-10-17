-- logical: 0055_gitea_task_claims — task-id mapping + tasks_version counter
-- for the Gitea `runner.v1` compatibility surface (actions parity Phase 5).
-- act_runner identifies in-flight work by a never-reused numeric task_id and
-- polls with a monotonic tasks_version watermark.

CREATE TABLE action_task_claims (
  task_id    BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
  job_id     VARCHAR(64) NOT NULL,
  runner_id  VARCHAR(64) NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  KEY idx_action_task_claims_job (job_id),
  CONSTRAINT fk_action_task_claims_job FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_task_claims_runner FOREIGN KEY (runner_id) REFERENCES action_runners(id) ON DELETE CASCADE
) ENGINE=InnoDB;

-- Single-row monotonic counters. `tasks` bumps on every claimable job
-- enqueue/requeue; FetchTask compares it against the runner's watermark.
CREATE TABLE action_counters (
  name  VARCHAR(64) PRIMARY KEY,
  value BIGINT NOT NULL DEFAULT 0
) ENGINE=InnoDB;
