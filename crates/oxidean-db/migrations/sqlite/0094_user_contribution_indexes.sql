-- logical: 0094_user_contribution_indexes — actor-scoped access paths for
-- user.getContributions (COL-12). repository_activity had no actor index at
-- all; pull_reviews only led with pull_id. Both also speed up the user Atom
-- feed and admin user-activity view.

CREATE INDEX IF NOT EXISTS idx_repository_activity_actor_created
  ON repository_activity(actor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_pull_reviews_author
  ON pull_reviews(author_id, submitted_at DESC);
