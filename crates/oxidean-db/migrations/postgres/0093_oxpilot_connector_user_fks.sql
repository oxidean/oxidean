-- logical: 0093_oxpilot_connector_user_fks — cascade connector rows
-- with their owner. `delete_user` issues a bare `DELETE FROM users`
-- and relies on ON DELETE CASCADE FKs everywhere; pairings/targets
-- lacked one, so a deleted user's verifier target stayed
-- authenticable (verifier_hash lookup returned the dead user_id).
-- Orphaned rows go first — ADD CONSTRAINT validates existing data;
-- targets' children cascade through their own target FKs.
DELETE FROM oxpilot_pairings WHERE user_id NOT IN (SELECT id FROM users);
DELETE FROM oxpilot_targets  WHERE user_id NOT IN (SELECT id FROM users);

ALTER TABLE oxpilot_pairings
    ADD CONSTRAINT fk_oxpilot_pairings_user
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE oxpilot_targets
    ADD CONSTRAINT fk_oxpilot_targets_user
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
