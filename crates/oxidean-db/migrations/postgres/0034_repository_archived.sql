-- logical: 0033_repository_archived — read-only archive mode (GIT-20)

ALTER TABLE repositories ADD COLUMN IF NOT EXISTS archived BOOLEAN NOT NULL DEFAULT false;
