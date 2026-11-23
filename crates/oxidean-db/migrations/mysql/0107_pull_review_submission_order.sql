ALTER TABLE pull_reviews ADD COLUMN submission_order BIGINT NULL;

CREATE TABLE pull_review_submission_order (
  id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY
) ENGINE=InnoDB;

CREATE UNIQUE INDEX idx_pull_reviews_submission_order
  ON pull_reviews(submission_order);