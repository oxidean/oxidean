-- actions/cache service storage (P14): committed cache entries keyed by
-- (repository, key, version, scope). GitHub scopes entries to a branch — a
-- save on the default branch is readable by sibling branches, while a
-- feature-branch save is only readable on that branch. `state` distinguishes
-- a reserved-but-uncommitted entry (POST /caches) from a finalized upload.
-- `lookup_hash` = sha256(key || '\n' || version || '\n' || scope) — keeps the
-- uniqueness index narrow enough for every dialect while retaining the
-- plaintext fields for listings and debugging.
CREATE TABLE action_caches (
  id               TEXT PRIMARY KEY,
  repository_id    TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  lookup_hash      TEXT NOT NULL,
  cache_key        TEXT NOT NULL,
  version          TEXT NOT NULL,
  scope            TEXT NOT NULL DEFAULT '',
  storage_path     TEXT NOT NULL,
  size_bytes       BIGINT NOT NULL DEFAULT 0,
  state            TEXT NOT NULL DEFAULT 'reserved',
  created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_accessed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (repository_id, lookup_hash)
);
-- Restore-key prefix matching scans keys per repo.
CREATE INDEX idx_action_caches_repo_key ON action_caches(repository_id, cache_key);
-- LRU eviction scans by repo + last_accessed_at.
CREATE INDEX idx_action_caches_repo_access ON action_caches(repository_id, last_accessed_at);

-- Per-job bearer tokens minted at claim for the `_apis/artifactcache`
-- endpoints (ACTIONS_RUNTIME_TOKEN). Hash stored; the plaintext is handed to
-- the claiming runner and never persisted.
CREATE TABLE action_cache_tokens (
  token_hash    TEXT PRIMARY KEY,
  job_id        TEXT NOT NULL REFERENCES action_jobs(id) ON DELETE CASCADE,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  expires_at    TIMESTAMPTZ NOT NULL,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_action_cache_tokens_job ON action_cache_tokens(job_id);
