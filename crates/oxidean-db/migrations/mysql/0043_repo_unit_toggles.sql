-- logical: 0043_repo_unit_toggles — per-repo unit enable flags (COL-13).
--
-- `issues_enabled` / `pulls_enabled` gate the `issue.*` / `pull.*` RPC
-- families and hide the Issues/Pulls tabs in repo chrome. Disabling a unit
-- never deletes data — rows stay put and RPCs resume when re-enabled.
--
-- Extending to future units (wiki, boards, releases, packages): add a
-- `<unit>_enabled` column in a follow-up migration, `repo.<unit>.getEnabled`
-- / `setEnabled` RPCs, a `repo.<unit>.disabled` gate at the unit's RPC acl
-- resolvers, and a chrome/route guard on the web side.
-- (`actions_enabled` predates this convention; it follows the same shape but
-- is managed by the Actions domain module.)

ALTER TABLE repositories ADD COLUMN issues_enabled TINYINT(1) NOT NULL DEFAULT 1;
ALTER TABLE repositories ADD COLUMN pulls_enabled TINYINT(1) NOT NULL DEFAULT 1;
