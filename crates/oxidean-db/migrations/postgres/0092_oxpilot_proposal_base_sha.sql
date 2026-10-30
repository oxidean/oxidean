-- U8 follow-up (oxpilot): `base_sha` pins the proposal's base to the
-- exact commit `canonical_diff` read its blobs from. `base` stays the
-- ref NAME — `pull.create.base_ref` needs a branch, and pre-0092 rows
-- (and proposals whose `session.branch` is already a commit-ish, so
-- `repo.refs` lists no match) backfill `''` meaning "no pin — use
-- `base` verbatim". `repo.branchCreate.start` prefers `base_sha` so
-- the apply tree can never drift from the tree the patch was
-- generated against if the named branch moves between propose and
-- apply. Append-only — applied migrations are never edited.

ALTER TABLE oxpilot_proposals
  ADD COLUMN base_sha TEXT NOT NULL DEFAULT '';
