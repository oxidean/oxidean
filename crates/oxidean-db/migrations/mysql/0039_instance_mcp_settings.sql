-- logical: 0039_instance_mcp_settings — instance MCP endpoint toggle (AGT-03).
-- enabled NULL = no admin override → the OXIDEAN_MCP_ENABLED env default
-- applies (same env-default/override split as instance_lfs_settings).
CREATE TABLE IF NOT EXISTS instance_mcp_settings (
  id          TINYINT     PRIMARY KEY CHECK (id = 1),
  enabled     BOOLEAN     NULL,
  updated_at  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
);

INSERT IGNORE INTO instance_mcp_settings (id) VALUES (1);
