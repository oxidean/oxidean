-- logical: 0039_instance_mcp_settings — instance MCP endpoint toggle (AGT-03).
-- enabled NULL = no admin override → the OXIDEAN_MCP_ENABLED env default
-- applies (same env-default/override split as instance_lfs_settings).
CREATE TABLE IF NOT EXISTS instance_mcp_settings (
  id          INTEGER     PRIMARY KEY CHECK (id = 1),
  enabled     INTEGER     NULL,
  updated_at  TEXT        NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S','now'))
);

INSERT OR IGNORE INTO instance_mcp_settings (id) VALUES (1);
