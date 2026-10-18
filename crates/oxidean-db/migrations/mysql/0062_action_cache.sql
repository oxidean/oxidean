-- actions/cache service storage (P14): committed cache entries keyed by
-- (repository, key, version, scope). GitHub scopes entries to a branch — a
-- save on the default branch is readable by sibling branches, while a
-- feature-branch save is only readable on that branch. `state` distinguishes
-- a reserved-but-uncommitted entry (POST /caches) from a finalized upload.
-- `lookup_hash` = sha256(key || '\n' || version || '\n' || scope) — keeps the
-- uniqueness index narrow enough for InnoDB's 3072-byte prefix limit while
-- retaining the plaintext fields for listings and debugging.
CREATE TABLE action_caches (
  id               VARCHAR(64) PRIMARY KEY,
  repository_id    CHAR(36) NOT NULL,
  lookup_hash      VARCHAR(64) NOT NULL,
  cache_key        VARCHAR(512) NOT NULL,
  version          VARCHAR(255) NOT NULL,
  scope            VARCHAR(255) NOT NULL DEFAULT '',
  storage_path     VARCHAR(1024) NOT NULL,
  size_bytes       BIGINT NOT NULL DEFAULT 0,
  state            VARCHAR(32) NOT NULL DEFAULT 'reserved',
  created_at       DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_accessed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (repository_id, lookup_hash),
  CONSTRAINT fk_action_caches_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;
-- Restore-key prefix matching scans keys per repo.
CREATE INDEX idx_action_caches_repo_key ON action_caches(repository_id, cache_key(255));
-- LRU eviction scans by repo + last_accessed_at.
CREATE INDEX idx_action_caches_repo_access ON action_caches(repository_id, last_accessed_at);

-- Per-job bearer tokens minted at claim for the `_apis/artifactcache`
-- endpoints (ACTIONS_RUNTIME_TOKEN). Hash stored; the plaintext is handed to
-- the claiming runner and never persisted.
CREATE TABLE action_cache_tokens (
  token_hash    VARCHAR(128) PRIMARY KEY,
  job_id        CHAR(36) NOT NULL,
  repository_id CHAR(36) NOT NULL,
  expires_at    DATETIME NOT NULL,
  created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_action_cache_tokens_job FOREIGN KEY (job_id) REFERENCES action_jobs(id) ON DELETE CASCADE,
  CONSTRAINT fk_action_cache_tokens_repo FOREIGN KEY (repository_id) REFERENCES repositories(id) ON DELETE CASCADE
) ENGINE=InnoDB;
CREATE INDEX idx_action_cache_tokens_job ON action_cache_tokens(job_id);
