-- logical: 0097_issue_pull_pins — pinned issues/PRs per repo (COL-04 repo
-- half, GitHub parity ≤3 each). `pinned_position` is the 0-based display
-- order; NULL = not pinned. Pin/unpin rewrites positions contiguously.

ALTER TABLE issues ADD COLUMN pinned_position INT NULL;
ALTER TABLE pull_requests ADD COLUMN pinned_position INT NULL;

CREATE INDEX idx_issues_repo_pinned
  ON issues(repo_id, pinned_position);
CREATE INDEX idx_pull_requests_repo_pinned
  ON pull_requests(repo_id, pinned_position);
