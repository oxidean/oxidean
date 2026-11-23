ALTER TABLE pull_reviews ADD COLUMN submission_order BIGINT NULL;

CREATE TABLE pull_review_submission_order (
  id TINYINT NOT NULL PRIMARY KEY,
  last_order BIGINT NOT NULL
) ENGINE=InnoDB;

INSERT INTO pull_review_submission_order (id, last_order) VALUES (1, 0);

CREATE UNIQUE INDEX idx_pull_reviews_submission_order
  ON pull_reviews(submission_order);