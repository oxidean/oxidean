ALTER TABLE action_runner_tokens
  ADD COLUMN purpose TEXT NOT NULL DEFAULT 'register';
