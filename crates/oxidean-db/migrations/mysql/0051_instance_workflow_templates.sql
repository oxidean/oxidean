-- logical: 0051_instance_workflow_templates — admin-managed instance workflow
-- templates surfaced in the actions.templates catalog alongside the vendored
-- builtin set and org `.github` repo templates.

CREATE TABLE IF NOT EXISTS instance_workflow_templates (
  id                 CHAR(36) PRIMARY KEY,
  slug               VARCHAR(191) NOT NULL,
  name               VARCHAR(255) NOT NULL,
  description        TEXT NOT NULL,
  icon_name          VARCHAR(191) NULL,
  categories_json    TEXT NOT NULL,
  file_patterns_json TEXT NOT NULL,
  filename           VARCHAR(255) NOT NULL,
  content            LONGTEXT NOT NULL,
  enabled            TINYINT(1) NOT NULL DEFAULT 1,
  created_by_user_id CHAR(36) NOT NULL,
  created_at         TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at         TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
  UNIQUE KEY uq_instance_workflow_templates_slug (slug),
  CONSTRAINT fk_instance_workflow_templates_user
    FOREIGN KEY (created_by_user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE INDEX idx_instance_workflow_templates_enabled
  ON instance_workflow_templates(enabled);
