-- logical: 0068_action_workflow_state — per-workflow disable flag.
--
-- GitHub lets a repo admin disable/enable a workflow file; a disabled
-- workflow does not run on any trigger (events, schedule, dispatch,
-- workflow_call). Rows exist only after a toggle — absent = enabled.

CREATE TABLE action_workflow_states (
  repository_id  CHAR(36) NOT NULL,
  path           VARCHAR(255) NOT NULL,
  disabled       TINYINT(1) NOT NULL DEFAULT 0,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  PRIMARY KEY (repository_id, path),
  CONSTRAINT fk_action_workflow_states_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;
