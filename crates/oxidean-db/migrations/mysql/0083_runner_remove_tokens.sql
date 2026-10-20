ALTER TABLE action_runner_tokens
  ADD COLUMN purpose VARCHAR(32) NOT NULL DEFAULT 'register';
