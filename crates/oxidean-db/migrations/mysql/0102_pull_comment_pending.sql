-- logical: 0102_pull_comment_pending — server-persisted pending review
-- comments ("drafts") on pull requests (WI-0001 follow-up). Pending comments
-- are visible only to their author until `pull.reviews.submit` links them,
-- which clears the flag.

ALTER TABLE pull_comments ADD COLUMN pending TINYINT(1) NOT NULL DEFAULT 0;
