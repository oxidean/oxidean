-- logical: 0084_action_artifacts_mime_type — backfill the column 0082 added
-- on sqlite/mysql but missed here. `mime_type` round-trips CreateArtifact's
-- content type so the signed-URL download can emit it (the v4 client picks
-- zip-extract vs raw on it).

ALTER TABLE action_artifacts ADD COLUMN mime_type TEXT NOT NULL DEFAULT 'application/zip';
