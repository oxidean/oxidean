-- logical: 0034_require_signed_commits — branch protection "require signed commits" (GIT-22)
-- Dialect SQL only.

ALTER TABLE branch_protection_rules
  ADD COLUMN require_signed_commits INTEGER NOT NULL DEFAULT 0;
