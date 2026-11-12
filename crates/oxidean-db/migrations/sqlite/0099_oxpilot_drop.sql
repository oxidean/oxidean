-- logical: 0099_oxpilot_drop — remove the ox-pilot feature (issue #258).
-- Drops every oxpilot_* table created by 0085-0093; indexes and FK
-- constraints die with their tables. Historical migrations stay —
-- already-applied databases replay them then drop here. Children drop
-- before parents (sessions -> targets FK).

-- 0093 left AFTER DELETE triggers ON users referencing oxpilot tables —
-- DROP TABLE does not remove triggers owned by other tables.
DROP TRIGGER IF EXISTS fk_oxpilot_pairings_user;
DROP TRIGGER IF EXISTS fk_oxpilot_targets_user;

DROP TABLE IF EXISTS oxpilot_queued_messages;
DROP TABLE IF EXISTS oxpilot_messages;
DROP TABLE IF EXISTS oxpilot_tool_calls;
DROP TABLE IF EXISTS oxpilot_actions;
DROP TABLE IF EXISTS oxpilot_proposals;
DROP TABLE IF EXISTS oxpilot_grants;
DROP TABLE IF EXISTS oxpilot_pairings;
DROP TABLE IF EXISTS oxpilot_sessions;
DROP TABLE IF EXISTS oxpilot_targets;
DROP TABLE IF EXISTS oxpilot_settings;
DROP TABLE IF EXISTS oxpilot_usage_counters;
