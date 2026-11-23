ALTER TABLE pull_reviews ADD COLUMN submission_order INTEGER NULL;

CREATE TABLE pull_review_submission_order (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  last_order INTEGER NOT NULL
);

INSERT INTO pull_review_submission_order (id, last_order) VALUES (1, 0);

CREATE UNIQUE INDEX idx_pull_reviews_submission_order
  ON pull_reviews(submission_order);