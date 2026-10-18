-- logical: 0054_vcs_key_collation — binary collation for VCS key columns
-- (u4-vcs-changeset-core, SR4.6).
--
-- Case-insensitive default collation PK-collides case-variant paths and ref
-- names (`README`/`readme` → silent upsert-merge = row loss) and makes the
-- `>` keyset cursor order non-bytewise. Every VCS key column is re-declared
-- under utf8mb4/utf8mb4_bin; `index_key` widens to VARCHAR(560) so the
-- `{at}\x01{hash32}\x01{chunk:04}`/tree/path-touch grammars fit inside the
-- (36+64+560)×4 = 2640B InnoDB PK budget (<3072).
-- Explicit CHARACTER SET accompanies every COLLATE — a bare COLLATE errors
-- when the column charset is not already utf8mb4 (0053 carried none).

ALTER TABLE vcs_derived_index
  MODIFY COLUMN index_key VARCHAR(560) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;

ALTER TABLE vcs_refs
  MODIFY COLUMN name VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;

ALTER TABLE vcs_file_changes
  MODIFY COLUMN path VARCHAR(512) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;

ALTER TABLE vcs_file_changes
  MODIFY COLUMN old_path VARCHAR(512) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL;
