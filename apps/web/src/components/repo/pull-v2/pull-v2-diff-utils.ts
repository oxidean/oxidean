import type { PullCommentPublic, PullDiffFile } from "@oxidean/api-client";

/**
 * pr-new-ui (WI-0001 S4–S7): pure helpers for the "Files changed" tab —
 * file-tree building, anchor ids, scoped-diff parsing, and line-anchor math
 * shared with the legacy diff viewers. No hooks live here (hook helpers
 * must stay in `.tsrx`).
 */

/** Diff scope resolved from the `/changes[/<scope>]` splat + local state. */
export type ChangesScope =
  | { kind: "none" }
  | { kind: "sha"; sha: string }
  | { kind: "range"; base: string; head: string }
  /** Client-only selection (no URL) — diffs the caller's last review..head. */
  | { kind: "since" };

/** `<sha>` or `<base>..<head>` (also `...`) → scope; `since` is applied by the caller. */
export function parseChangesScope(changesSha?: string | null): ChangesScope {
  if (!changesSha) return { kind: "none" };
  const i = changesSha.indexOf("..");
  if (i >= 0) {
    const base = changesSha.slice(0, i);
    const head = changesSha.slice(i + 2).replace(/^\.+/, "");
    if (base && head) return { kind: "range", base, head };
    return { kind: "none" };
  }
  return { kind: "sha", sha: changesSha };
}

/** Query-key discriminator for the scoped `pull.files` call. */
export function changesScopeKey(scope: ChangesScope): string {
  switch (scope.kind) {
    case "sha":
      return `sha:${scope.sha}`;
    case "range":
      return `range:${scope.base}..${scope.head}`;
    case "since":
      return "since";
    default:
      return "all";
  }
}

/** Head sha a new comment anchors to under the current diff scope. */
export function scopeAnchorSha(scope: ChangesScope, headSha?: string | null): string | null {
  if (scope.kind === "sha") return scope.sha;
  if (scope.kind === "range") return scope.head;
  return headSha ?? null;
}

/** Where an inline draft box is open. Null fields = file-level comment. */
export type DraftTarget = {
  path: string;
  /** Diff row the composer docks under — `null` docks under the file header. */
  row: number | null;
  side: "LEFT" | "RIGHT" | null;
  line: number | null;
  /** Range start (`start_line`) when the draft came from a gutter drag. */
  startLine?: number | null;
};

/** Line number a `side` anchors to on a parsed diff row, else `null`. */
export function anchorLineForSide(
  kind: string,
  oldNo: number | null,
  newNo: number | null,
  side: "LEFT" | "RIGHT",
): number | null {
  if (side === "LEFT") {
    return (kind === "del" || kind === "ctx") && oldNo != null ? oldNo : null;
  }
  return (kind === "add" || kind === "ctx") && newNo != null ? newNo : null;
}

/** "commented on lines X–Y" for range comments; `null` for single-line. */
export function commentRangeLabel(c: PullCommentPublic): string | null {
  if (c.start_line == null || c.line == null) return null;
  return `commented on lines ${c.start_line}–${c.line}`;
}

/** True when the comment's [start_line..line] range covers this row. */
export function commentCoversRow(
  c: PullCommentPublic,
  kind: string,
  oldNo: number | null,
  newNo: number | null,
): boolean {
  if (c.start_line == null || c.line == null || !c.side) return false;
  const n = anchorLineForSide(kind, oldNo, newNo, c.side === "LEFT" ? "LEFT" : "RIGHT");
  return n != null && n >= c.start_line && n <= c.line;
}

/** `#pull-v2-file-<hash>` anchor id for a diff file block. */
export function fileAnchorId(path: string): string {
  let h = 5381;
  for (let i = 0; i < path.length; i++) {
    h = ((h << 5) + h + path.charCodeAt(i)) | 0;
  }
  return `pull-v2-file-${(h >>> 0).toString(36)}`;
}

function pathBasename(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? path : path.slice(i + 1);
}

function pathDirname(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

export type FileTreeDir = {
  /** Full directory path relative to the repo root ("" for root). */
  path: string;
  /** Display label — single-child chains collapse to "src/lib". */
  label: string;
  dirs: FileTreeDir[];
  files: PullDiffFile[];
};

/**
 * Build a directory trie from flat diff paths and collapse chains of
 * directories that contain a single sub-directory (GitHub-style
 * "src/lib/parser" rows). Directories sort first, alphabetically; file
 * order inside a directory keeps the server's diff order.
 */
export function buildFileTree(files: PullDiffFile[]): FileTreeDir {
  const root: FileTreeDir = { path: "", label: "", dirs: [], files: [] };
  const byPath = new Map<string, FileTreeDir>([["", root]]);
  for (const f of files) {
    const dir = pathDirname(f.path);
    if (!dir) {
      root.files.push(f);
      continue;
    }
    let cur = root;
    let curPath = "";
    for (const seg of dir.split("/")) {
      curPath = curPath ? `${curPath}/${seg}` : seg;
      let next = byPath.get(curPath);
      if (!next) {
        next = { path: curPath, label: seg, dirs: [], files: [] };
        byPath.set(curPath, next);
        cur.dirs.push(next);
      }
      cur = next;
    }
    cur.files.push(f);
  }
  const collapse = (d: FileTreeDir) => {
    for (const child of d.dirs) {
      while (child.dirs.length === 1 && child.files.length === 0) {
        const only = child.dirs[0]!;
        child.label = `${child.label}/${only.label}`;
        child.path = only.path;
        child.dirs = only.dirs;
        child.files = only.files;
      }
      collapse(child);
    }
    d.dirs.sort((a, b) => a.label.localeCompare(b.label));
  };
  collapse(root);
  return root;
}

/**
 * Flattened pre-order tree row for the changes-tab sidebar. Single flat
 * shape (not a discriminated union) — `@if`/`@else` arms don't narrow
 * unions across directive boundaries.
 */
export type FileTreeRow = {
  kind: "dir" | "file";
  key: string;
  /** Directory path, or the file's full path. */
  path: string;
  /** Directory label (collapsed "src/lib"), or the file basename. */
  label: string;
  depth: number;
  /** Directories only: children currently hidden. */
  collapsed: boolean;
  /** Files only — `null` on directory rows. */
  file: PullDiffFile | null;
};

/**
 * Pre-order flatten of {@link buildFileTree} honoring `collapsed` dir paths —
 * rendering stays a flat `@for` (no recursive template arms).
 */
export function flattenFileTree(
  dir: FileTreeDir,
  collapsed: ReadonlySet<string>,
  depth = -1,
): FileTreeRow[] {
  const rows: FileTreeRow[] = [];
  for (const sub of dir.dirs) {
    const isCollapsed = collapsed.has(sub.path);
    rows.push({
      kind: "dir",
      key: `d:${sub.path}`,
      path: sub.path,
      label: sub.label,
      depth: depth + 1,
      collapsed: isCollapsed,
      file: null,
    });
    if (!isCollapsed) rows.push(...flattenFileTree(sub, collapsed, depth + 1));
  }
  for (const f of dir.files) {
    rows.push({
      kind: "file",
      key: `f:${f.path}`,
      path: f.path,
      label: pathBasename(f.path),
      depth: depth + 1,
      collapsed: false,
      file: f,
    });
  }
  return rows;
}

/** Diff-line status → row tinting classes shared by unified/split rows. */
export function diffLineClass(kind: string): string {
  if (kind === "add") return "bg-[var(--diff-add-bg)]";
  if (kind === "del") return "bg-[var(--diff-del-bg)]";
  if (kind === "hunk") return "bg-[var(--diff-hunk-bg)] text-[var(--diff-hunk-fg)]";
  if (kind === "meta" || kind === "note") return "text-muted-foreground";
  return "text-foreground";
}

/** File status → tree/icon color + short label. */
export function fileStatusMeta(status: string): { label: string; className: string } {
  switch (status) {
    case "added":
      return { label: "A", className: "text-success" };
    case "deleted":
    case "removed":
      return { label: "D", className: "text-destructive" };
    case "renamed":
      return { label: "R", className: "text-amber-500" };
    case "modified":
    default:
      return { label: "M", className: "text-amber-500" };
  }
}
