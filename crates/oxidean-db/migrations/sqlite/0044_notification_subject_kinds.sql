-- logical: 0044_notification_subject_kinds — widen notification subjects to
-- `release` / `workflow_run` (DEBT-06 follow-ups): adds nullable `subject_ref`
-- (release tag / run id for deep links) and the `completion_notified` claim
-- flag on action_runs so run-completion notifications fire at most once.

-- Table rebuild for the CHECK change: nothing references `notifications`, so
-- drop/copy/rename is safe with FK enforcement left on (PRAGMA foreign_keys
-- cannot toggle inside sqlx's per-migration transaction — omit it).
CREATE TABLE notifications_new (
  id               TEXT PRIMARY KEY,
  recipient_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  actor_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  reason           TEXT NOT NULL,
  subject_kind     TEXT NOT NULL,
  subject_repo_id  TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  subject_number   INTEGER NOT NULL,
  subject_title    TEXT NOT NULL DEFAULT '',
  subject_ref      TEXT NULL,
  read_at          TEXT NULL,
  created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  CONSTRAINT notifications_subject_kind_check CHECK (subject_kind IN ('issue', 'pull_request', 'release', 'workflow_run', 'push'))
);
INSERT INTO notifications_new
  SELECT id, recipient_id, actor_id, reason, subject_kind, subject_repo_id,
         subject_number, subject_title, NULL, read_at, created_at
  FROM notifications;
DROP TABLE notifications;
ALTER TABLE notifications_new RENAME TO notifications;
CREATE INDEX IF NOT EXISTS idx_notifications_recipient_created
  ON notifications (recipient_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_recipient_unread
  ON notifications (recipient_id, created_at DESC)
  WHERE read_at IS NULL;

ALTER TABLE action_runs ADD COLUMN completion_notified INTEGER NOT NULL DEFAULT 0;
