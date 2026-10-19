-- u6 git-interop: import `active`/`cursor` marker rows exist before any
-- changeset is minted, so covering_changeset must admit NULL. MODIFY keeps
-- the fk_vcs_didx_cs FK + idx_vcs_derived_index_covering automatically.
ALTER TABLE vcs_derived_index
  MODIFY covering_changeset VARCHAR(32) NULL;
