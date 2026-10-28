-- U5 (oxpilot-tools): `ops_json` stores the serialized ordered DiffFileOp
-- list the apply path executes verbatim — `diff_body` alone cannot carry
-- deletes/renames/exact post-image bytes. Append-only column add; 0087 is
-- never edited. `[]` backfills pre-existing rows.

ALTER TABLE oxpilot_proposals ADD COLUMN ops_json TEXT NOT NULL DEFAULT '[]';
