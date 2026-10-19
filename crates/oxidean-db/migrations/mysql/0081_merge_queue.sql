-- logical: 0078_merge_queue — merge queue entries + merge groups (GitHub
-- "Require merge queue" branch protection + `merge_group` lifecycle event).
--
-- `merge_queue_entries` is the FIFO train: `position` is enqueue order within
-- a repo+base_ref. `state` is `queued` → `merged` | `dequeued` (removed —
-- manual dequeue, failing checks, head moved, PR closed). A pull holds at
-- most one `queued` row — MySQL has no partial indexes, so `queued_pull`
-- materializes pull_id only while queued (NULLs never collide). MySQL also
-- rejects generated columns whose expression references an FK column (1215),
-- so `queued_pull`/`checking_key` are maintained by the writer, not GENERATED.
--
-- `merge_groups` is the tested batch: the front group spans base_tip + every
-- `queued` entry in position order, materialized on
-- `refs/heads/gh-readonly-queue/{base}/…` as a chain of merge commits —
-- exactly the tree Actions `on: merge_group` runs check out. `state` is
-- `checking` → `merged` | `destroyed`. The group's `head_sha` fast-forwards
-- `base_ref` on success (the tested chain becomes history, GitHub parity).
--
-- `required_merge_queue` on branch_protection_rules is GitHub's ruleset
-- option: matching branches reject direct `pulls.merge` and route merges
-- through the queue.

ALTER TABLE branch_protection_rules
  ADD COLUMN required_merge_queue TINYINT(1) NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS merge_groups (
  id         CHAR(36) PRIMARY KEY,
  repo_id    CHAR(36) NOT NULL,
  base_ref   VARCHAR(255) NOT NULL,
  base_sha   VARCHAR(64) NOT NULL DEFAULT '',
  head_ref   VARCHAR(255) NOT NULL DEFAULT '',
  head_sha   VARCHAR(64) NOT NULL DEFAULT '',
  state      VARCHAR(16) NOT NULL DEFAULT 'checking',
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT chk_merge_groups_state CHECK (state IN ('checking', 'merged', 'destroyed')),
  checking_key VARCHAR(320) NULL,
  CONSTRAINT fk_merge_groups_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  UNIQUE KEY uk_merge_groups_checking (checking_key),
  KEY merge_groups_repo_state_idx (repo_id, base_ref, state)
);

CREATE TABLE IF NOT EXISTS merge_queue_entries (
  id           CHAR(36) PRIMARY KEY,
  repo_id      CHAR(36) NOT NULL,
  pull_id      CHAR(36) NOT NULL,
  pull_number  BIGINT NOT NULL,
  base_ref     VARCHAR(255) NOT NULL,
  position     BIGINT NOT NULL,
  state        VARCHAR(16) NOT NULL DEFAULT 'queued',
  group_id     CHAR(36) NULL,
  enqueued_by  CHAR(36) NOT NULL DEFAULT '',
  chain_sha    CHAR(40) NOT NULL DEFAULT '',
  dequeue_reason VARCHAR(255) NOT NULL DEFAULT '',
  queued_pull  CHAR(36) NULL,
  created_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at   TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT chk_merge_queue_entries_state CHECK (state IN ('queued', 'merged', 'dequeued')),
  CONSTRAINT fk_mq_entries_repo FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE,
  CONSTRAINT fk_mq_entries_pull FOREIGN KEY (pull_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  CONSTRAINT fk_mq_entries_group FOREIGN KEY (group_id) REFERENCES merge_groups(id) ON DELETE SET NULL,
  UNIQUE KEY uk_mq_entries_queued_pull (queued_pull),
  UNIQUE KEY uk_mq_entries_position (repo_id, base_ref, position),
  KEY merge_queue_entries_repo_idx (repo_id, base_ref, state, position)
);
