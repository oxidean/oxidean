-- u6 git-interop: import `active`/`cursor` marker rows exist before any
-- changeset is minted, so covering_changeset must admit NULL. The FK +
-- covering index ride through; DROP NOT NULL keeps both automatically.
ALTER TABLE vcs_derived_index
  ALTER COLUMN covering_changeset DROP NOT NULL;
