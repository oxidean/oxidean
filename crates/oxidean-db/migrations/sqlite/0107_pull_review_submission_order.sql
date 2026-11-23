ALTER TABLE pull_reviews ADD COLUMN submission_order INTEGER NULL;

CREATE TABLE pull_review_submission_order (
  id INTEGER PRIMARY KEY AUTOINCREMENT
);

CREATE UNIQUE INDEX idx_pull_reviews_submission_order
  ON pull_reviews(submission_order);