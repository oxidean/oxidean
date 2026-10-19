-- logical: 0078_merge_queue — merge queue entries + merge groups (GitHub
-- "Require merge queue" branch protection + `merge_group` lifecycle event).
--
-- `merge_queue_entries` is the FIFO train: `position` is enqueue order within
-- a repo+base_ref. `state` is `queued` → `merged` | `dequeued` (removed —
-- manual dequeue, failing checks, head moved, PR closed). A pull holds at
-- most one `queued` row (partial unique index).
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
  ADD COLUMN required_merge_queue INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS merge_groups (
  id         TEXT PRIMARY KEY,
  repo_id    TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  base_ref   TEXT NOT NULL,
  base_sha   TEXT NOT NULL DEFAULT '',
  head_ref   TEXT NOT NULL DEFAULT '',
  head_sha   TEXT NOT NULL DEFAULT '',
  state      TEXT NOT NULL DEFAULT 'checking'
    CHECK (state IN ('checking', 'merged', 'destroyed')),
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
CREATE INDEX IF NOT EXISTS merge_groups_repo_state_idx
  ON merge_groups (repo_id, base_ref, state);
-- one in-flight verification batch per repo+base: concurrent enqueues race
-- `checking_group_for_base` → `create_merge_group`; the loser hits this
-- index and treats the winner's group as the batch to ride.
CREATE UNIQUE INDEX IF NOT EXISTS merge_groups_checking_idx
  ON merge_groups (repo_id, base_ref) WHERE state = 'checking';

CREATE TABLE IF NOT EXISTS merge_queue_entries (
  id           TEXT PRIMARY KEY,
  repo_id      TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  pull_id      TEXT NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
  pull_number  INTEGER NOT NULL,
  base_ref     TEXT NOT NULL,
  position     INTEGER NOT NULL,
  state        TEXT NOT NULL DEFAULT 'queued'
    CHECK (state IN ('queued', 'merged', 'dequeued')),
  group_id     TEXT REFERENCES merge_groups(id) ON DELETE SET NULL,
  enqueued_by  TEXT NOT NULL DEFAULT '',
  chain_sha    TEXT NOT NULL DEFAULT '',
  dequeue_reason TEXT NOT NULL DEFAULT '',
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now')),
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS merge_queue_entries_pull_live_idx
  ON merge_queue_entries (pull_id) WHERE state = 'queued';
CREATE UNIQUE INDEX IF NOT EXISTS merge_queue_entries_position_idx
  ON merge_queue_entries (repo_id, base_ref, position);
CREATE INDEX IF NOT EXISTS merge_queue_entries_repo_idx
  ON merge_queue_entries (repo_id, base_ref, state, position);
