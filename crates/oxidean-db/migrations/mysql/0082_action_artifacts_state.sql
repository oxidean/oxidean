-- logical: 0082_action_artifacts_state — reserve→commit lifecycle for the
-- ArtifactService Twirp surface (`upload-artifact@v4` on external runners).
-- Builtin uploads keep inserting complete rows ('committed'); the Twirp
-- create/finalize pair parks a 'pending' row so the artifact name is reserved
-- before the blob lands, matching GitHub's create-time name reservation.
-- `mime_type` round-trips CreateArtifact's content type so the signed-URL
-- download can emit it (the v4 client picks zip-extract vs raw on it).

ALTER TABLE action_artifacts ADD COLUMN state VARCHAR(32) NOT NULL DEFAULT 'committed';

ALTER TABLE action_artifacts ADD COLUMN mime_type VARCHAR(255) NOT NULL DEFAULT 'application/zip';

CREATE INDEX idx_action_artifacts_state_updated
  ON action_artifacts(state, updated_at);
