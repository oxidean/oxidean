-- U5 (oxpilot-tools): `ops_json` stores the serialized ordered DiffFileOp
-- list the apply path executes verbatim — `diff_body` alone cannot carry
-- deletes/renames/exact post-image bytes. Append-only column add; 0087 is
-- never edited. MEDIUMTEXT cannot take a literal default — existing rows
-- backfill to '' and readers treat empty as an empty op list.

ALTER TABLE oxpilot_proposals ADD COLUMN ops_json MEDIUMTEXT NOT NULL;
