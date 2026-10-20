-- logical: 0083_runner_remove_tokens — `purpose` on action_runner_tokens
-- separates registration credentials ('register', default) from GitHub's
-- remove-token ('remove'): `POST …/actions/runners/remove-token` mints a
-- one-shot credential that `DELETE …/actions/runners/{id}` accepts in place
-- of a user session, so `config.sh remove` works without user auth. Sharing
-- one table keeps the consume paths (UPDATE … RETURNING) uniform.

ALTER TABLE action_runner_tokens
  ADD COLUMN purpose TEXT NOT NULL DEFAULT 'register';
