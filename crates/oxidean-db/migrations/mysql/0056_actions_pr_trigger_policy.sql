-- logical: 0056_actions_pr_trigger_policy — per-repo Actions trigger policy
-- (actions parity Phase 6).
--
-- `actions_pull_request_target` opts the repository into `pull_request_target`
-- dispatch (trusted-context runs on base-branch workflows — dangerous on
-- hostile forks, hence explicit opt-in, default off).
-- `actions_fork_pr_approval` gates fork `pull_request`/`pull_request_target`
-- runs behind maintainer approval: 'never' (auto-run), 'first-time' (approve
-- unless the actor has repo write access or a prior completed run), 'always'
-- (every fork run waits). `first-time` is GitHub's "Require approval for
-- first-time contributors" analogue.

ALTER TABLE repositories ADD COLUMN actions_pull_request_target TINYINT(1) NOT NULL DEFAULT 0;
ALTER TABLE repositories ADD COLUMN actions_fork_pr_approval VARCHAR(16) NOT NULL DEFAULT 'never';

-- Approval hold flags — `waiting_approval` is a derived public status, not a
-- stored one: the jobs CHECK vocabulary predates it, and a flag keeps the
-- claim/scheduler predicates additive (approval_hold = 0). A held job keeps
-- `status='waiting'` (needs-evaluation shape preserved); a held run keeps its
-- natural status while the API coalesces hold → 'waiting_approval'.
ALTER TABLE action_runs ADD COLUMN approval_hold TINYINT(1) NOT NULL DEFAULT 0;
ALTER TABLE action_jobs ADD COLUMN approval_hold TINYINT(1) NOT NULL DEFAULT 0;

-- Cron sweeper dedupe — the sweeper fires `schedule` workflows at most once
-- per (repo, workflow, minute) tick, persisted so a restart inside the same
-- minute does not double-fire. Single-node semantics: the sweeper only runs
-- where the scheduler loop runs.
CREATE TABLE action_schedule_state (
  repository_id  VARCHAR(64) NOT NULL,
  workflow_path  VARCHAR(255) NOT NULL,
  cron           VARCHAR(128) NOT NULL,
  last_fired_minute BIGINT NOT NULL,
  PRIMARY KEY (repository_id, workflow_path, cron)
);
