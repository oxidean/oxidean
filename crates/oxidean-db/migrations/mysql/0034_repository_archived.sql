-- logical: 0033_repository_archived — read-only archive mode (GIT-20)
-- MySQL: no IF NOT EXISTS on ADD COLUMN — idempotent via migrate runner / fresh DBs.

ALTER TABLE repositories ADD COLUMN archived TINYINT(1) NOT NULL DEFAULT 0;
