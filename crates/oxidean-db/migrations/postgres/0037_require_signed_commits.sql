-- logical: 0034_require_signed_commits — branch protection "require signed commits" (GIT-22)

ALTER TABLE branch_protection_rules
  ADD COLUMN IF NOT EXISTS require_signed_commits BOOLEAN NOT NULL DEFAULT false;
