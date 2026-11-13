-- logical: 0103_pull_milestones — pull_requests.milestone_id, mirroring
-- issues.milestone_id (0074). Cleared when the milestone row is deleted;
-- milestone open/closed counts union issues + pull_requests.

ALTER TABLE pull_requests ADD COLUMN milestone_id CHAR(36) NULL,
  ADD CONSTRAINT fk_pull_requests_milestone FOREIGN KEY (milestone_id) REFERENCES milestones(id) ON DELETE SET NULL;
CREATE INDEX idx_pull_requests_milestone_id ON pull_requests(milestone_id);
