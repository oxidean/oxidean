-- logical: 0093_oxpilot_connector_user_fks — cascade connector rows
-- with their owner. `delete_user` issues a bare `DELETE FROM users`
-- and relies on ON DELETE CASCADE FKs everywhere; pairings/targets
-- lacked one, so a deleted user's verifier target stayed
-- authenticable (verifier_hash lookup returned the dead user_id).
-- SQLite cannot add a FK post-hoc, and the table-rebuild alternative
-- would implicit-DELETE under this pool's `PRAGMA foreign_keys = ON`
-- (wiping grants/actions through their target cascades). AFTER DELETE
-- triggers carry the same runtime guarantee; the targets delete still
-- cascades into grants/actions through their existing target FKs.
-- Orphaned rows go first — nothing dangling may survive.
DELETE FROM oxpilot_pairings WHERE user_id NOT IN (SELECT id FROM users);
DELETE FROM oxpilot_targets  WHERE user_id NOT IN (SELECT id FROM users);

CREATE TRIGGER fk_oxpilot_pairings_user
  AFTER DELETE ON users
  BEGIN
    DELETE FROM oxpilot_pairings WHERE user_id = OLD.id;
  END;

CREATE TRIGGER fk_oxpilot_targets_user
  AFTER DELETE ON users
  BEGIN
    DELETE FROM oxpilot_targets WHERE user_id = OLD.id;
  END;
