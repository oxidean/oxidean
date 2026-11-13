-- logical: 0103_pull_milestones — pull_requests.milestone_id, mirroring
-- issues.milestone_id (0074). Cleared when the milestone row is deleted;
-- milestone open/closed counts union issues + pull_requests.

ALTER TABLE pull_requests ADD COLUMN milestone_id TEXT NULL REFERENCES milestones(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_pull_requests_milestone_id ON pull_requests(milestone_id);
