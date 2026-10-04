-- logical: 0039_instance_mcp_settings — instance MCP endpoint toggle (AGT-03).
-- enabled NULL = no admin override → the OXIDEAN_MCP_ENABLED env default
-- applies (same env-default/override split as instance_lfs_settings).
CREATE TABLE IF NOT EXISTS instance_mcp_settings (
  id          SMALLINT    PRIMARY KEY CHECK (id = 1),
  enabled     BOOLEAN     NULL,
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO instance_mcp_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
