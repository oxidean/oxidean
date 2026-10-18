-- logical: 0068_action_workflow_state — per-workflow disable flag.
--
-- GitHub lets a repo admin disable/enable a workflow file; a disabled
-- workflow does not run on any trigger (events, schedule, dispatch,
-- workflow_call). Rows exist only after a toggle — absent = enabled.

CREATE TABLE action_workflow_states (
  repository_id  TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  path           TEXT NOT NULL,
  disabled       BOOLEAN NOT NULL DEFAULT FALSE,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (repository_id, path)
);
