-- U8 (oxpilot-ui): `computer_use_target_id` is the optional session→
-- target binding picked in the new-session dialog
-- (`session_create{target_id?}`); NULL stays "no target", the shipped
-- default. `oxpilot_targets` is the referent U1's deferral waited for
-- (0088); `ON DELETE SET NULL` so a tombstoned target never orphans the
-- binding. Append-only — 0087/0088 are never edited.

ALTER TABLE oxpilot_sessions
  ADD COLUMN computer_use_target_id TEXT NULL REFERENCES oxpilot_targets(id) ON DELETE SET NULL;
