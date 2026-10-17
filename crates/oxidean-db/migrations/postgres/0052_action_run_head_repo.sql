-- logical: 0052_action_run_head_repo — recorded PR head-repo provenance on
-- action_runs. Set at dispatch for pull_request events; NULL for push /
-- workflow_dispatch. Claim checks head_repo_id == repository_id to decide
-- whether the run is trusted for secrets and a write-capable job token.

ALTER TABLE action_runs ADD COLUMN IF NOT EXISTS head_repo_id TEXT NULL;
