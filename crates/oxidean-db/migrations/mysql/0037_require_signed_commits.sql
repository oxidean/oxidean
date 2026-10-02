-- logical: 0034_require_signed_commits — branch protection "require signed commits" (GIT-22)
-- MySQL: no IF NOT EXISTS on ADD COLUMN — idempotent via migrate runner / fresh DBs.

ALTER TABLE branch_protection_rules
  ADD COLUMN require_signed_commits TINYINT(1) NOT NULL DEFAULT 0;
