-- logical: 0051_instance_workflow_templates — admin-managed instance workflow
-- templates surfaced in the actions.templates catalog alongside the vendored
-- builtin set and org `.github` repo templates.

CREATE TABLE IF NOT EXISTS instance_workflow_templates (
  id                 TEXT PRIMARY KEY,
  slug               TEXT NOT NULL UNIQUE,
  name               TEXT NOT NULL,
  description        TEXT NOT NULL DEFAULT '',
  icon_name          TEXT NULL,
  categories_json    TEXT NOT NULL DEFAULT '[]',
  file_patterns_json TEXT NOT NULL DEFAULT '[]',
  filename           TEXT NOT NULL,
  content            TEXT NOT NULL,
  enabled            BOOLEAN NOT NULL DEFAULT true,
  created_by_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_instance_workflow_templates_enabled
  ON instance_workflow_templates(enabled);
