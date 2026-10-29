-- logical: 0091_oxpilot_text_defaults — restore the `DEFAULT ''` / `'[]'`
-- contract the postgres and sqlite DDL gives the oxpilot body-bearing
-- TEXT columns. Inserts may legitimately omit them: a `running`
-- `oxpilot_tool_calls` row has no output yet, a pre-`ops_json`
-- `oxpilot_proposals` row backfills `'[]'`, and refusal/queue rows can
-- carry empty bodies. Postgres/sqlite declare the defaults in
-- 0085/0087/0088/0089; MySQL's TEXT-family columns take only
-- *expression* defaults (8.0.13+), so the original columns shipped
-- NOT NULL without one and the omitting inserts fail with 1364.

ALTER TABLE oxpilot_messages
  ALTER COLUMN content SET DEFAULT ('');

ALTER TABLE oxpilot_tool_calls
  ALTER COLUMN args_summary SET DEFAULT (''),
  ALTER COLUMN output_summary SET DEFAULT ('');

ALTER TABLE oxpilot_queued_messages
  ALTER COLUMN content SET DEFAULT ('');

ALTER TABLE oxpilot_proposals
  ALTER COLUMN diff_body SET DEFAULT (''),
  ALTER COLUMN ops_json SET DEFAULT ('[]');

ALTER TABLE oxpilot_actions
  ALTER COLUMN args_summary SET DEFAULT ('');
