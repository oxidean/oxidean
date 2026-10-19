-- logical: 0074_wiki — `wiki_enabled` per-repo unit flag (COL-13 extension).
--
-- The wiki itself is a git-backed unit: pages live as Markdown files in the
-- `<repo>.wiki.git` sibling bare (GitHub/Gitea layout — cloneable, history
-- is git log). The flag only gates the `wiki.*` RPC surface and the Wiki
-- tab; the bare repo and its contents are never deleted by toggling off.

ALTER TABLE repositories ADD COLUMN wiki_enabled TINYINT(1) NOT NULL DEFAULT 1;
