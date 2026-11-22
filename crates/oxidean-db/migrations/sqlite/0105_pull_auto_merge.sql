-- logical: 0105_pull_auto_merge — GitHub-style auto-merge-when-green on
-- plain PRs (COL-07 tail). `auto_merge_method IS NOT NULL` = armed; the
-- armed method drives the fire-time merge. `auto_merge_set_by` records the
-- enabling user (merge attribution + disable ACL). Commit title/message
-- ride along for GitHub GraphQL input parity. `repositories.allow_auto_merge`
-- is the repo-level "Allow auto-merge" toggle next to the strategy flags.

ALTER TABLE pull_requests
  ADD COLUMN auto_merge_method TEXT NULL;
ALTER TABLE pull_requests
  ADD COLUMN auto_merge_set_by TEXT NULL;
ALTER TABLE pull_requests
  ADD COLUMN auto_merge_commit_title TEXT NULL;
ALTER TABLE pull_requests
  ADD COLUMN auto_merge_commit_message TEXT NULL;

ALTER TABLE repositories
  ADD COLUMN allow_auto_merge INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_pull_requests_auto_merge_head
  ON pull_requests(repo_id, head_sha) WHERE auto_merge_method IS NOT NULL;
