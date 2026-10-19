-- logical: 0068_action_workflow_state — per-workflow disable flag.
--
-- GitHub lets a repo admin disable/enable a workflow file; a disabled
-- workflow does not run on any trigger (events, schedule, dispatch,
-- workflow_call). Rows exist only after a toggle — absent = enabled.

CREATE TABLE action_workflow_states (
  repository_id  TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  path           TEXT NOT NULL,
  disabled       INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  PRIMARY KEY (repository_id, path)
);
