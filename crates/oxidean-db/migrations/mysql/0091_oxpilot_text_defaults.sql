-- logical: 0091_oxpilot_text_defaults — restore the `DEFAULT ''` / `'[]'`
-- contract the postgres and sqlite DDL gives the oxpilot body-bearing
-- TEXT columns. Inserts may legitimately omit them: a `running`
-- `oxpilot_tool_calls` row has no output yet, a pre-`ops_json`
-- `oxpilot_proposals` row backfills `'[]'`, and refusal/queue rows can
-- carry empty bodies. Postgres/sqlite declare the defaults in
-- 0085/0087/0088/0089; MySQL's TEXT-family columns take only
-- *expression* defaults (8.0.13+), so the original columns shipped
-- NOT NULL without one and the omitting inserts fail with 1364.
-- `MODIFY COLUMN` restates the definition (NOT NULL + charset-implicit
-- attributes preserved) — the parenthesised literal is stored as an
-- expression default, which TEXT columns require.

ALTER TABLE oxpilot_messages
  MODIFY COLUMN content MEDIUMTEXT NOT NULL DEFAULT ('');

ALTER TABLE oxpilot_tool_calls
  MODIFY COLUMN args_summary MEDIUMTEXT NOT NULL DEFAULT (''),
  MODIFY COLUMN output_summary MEDIUMTEXT NOT NULL DEFAULT ('');

ALTER TABLE oxpilot_queued_messages
  MODIFY COLUMN content MEDIUMTEXT NOT NULL DEFAULT ('');

ALTER TABLE oxpilot_proposals
  MODIFY COLUMN diff_body MEDIUMTEXT NOT NULL DEFAULT (''),
  MODIFY COLUMN ops_json MEDIUMTEXT NOT NULL DEFAULT ('[]');

ALTER TABLE oxpilot_actions
  MODIFY COLUMN args_summary MEDIUMTEXT NOT NULL DEFAULT ('');
