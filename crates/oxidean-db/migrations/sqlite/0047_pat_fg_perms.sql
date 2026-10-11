-- logical: 0047_pat_fg_perms — fine-grained permission families as JSON
-- ({"issues":"write","pull_requests":"read",…}); NULL = legacy token.
ALTER TABLE personal_access_tokens ADD COLUMN perms_json TEXT NULL;
