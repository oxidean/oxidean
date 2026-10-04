-- logical: 0044_notification_subject_kinds — widen notification subjects to
-- `release` / `workflow_run` (DEBT-06 follow-ups): adds nullable `subject_ref`
-- (release tag / run id for deep links) and the `completion_notified` claim
-- flag on action_runs so run-completion notifications fire at most once.

ALTER TABLE notifications DROP CHECK notifications_subject_kind_check;
ALTER TABLE notifications ADD CONSTRAINT notifications_subject_kind_check
  CHECK (subject_kind IN ('issue', 'pull_request', 'release', 'workflow_run', 'push'));
ALTER TABLE notifications ADD COLUMN subject_ref VARCHAR(255) NULL;

ALTER TABLE action_runs ADD COLUMN completion_notified TINYINT(1) NOT NULL DEFAULT 0;
