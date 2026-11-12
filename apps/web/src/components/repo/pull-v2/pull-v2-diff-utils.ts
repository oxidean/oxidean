import type { PullDiffFile } from "@oxidean/api-client";

/**
 * pr-new-ui (WI-0001 S4–S7): pure helpers for the "Files changed" tab —
 * file-tree building, anchor ids, and the local pending-review comment
 * model. No hooks live here (hook helpers must stay in `.tsrx`).
 */

/** A comment collected locally, submitted with the next review. */
export type PendingLineComment = {
  /** Local id (monotonic counter), not the server comment id. */
  id: number;
  path: string;
  /** Row index inside the file's parsed diff rows — display anchor. */
  row: number;
  side: "LEFT" | "RIGHT";
  line: number;
  body: string;
};

/** Where an inline draft box is open. */
export type DraftTarget = {
  path: string;
  row: number;
  side: "LEFT" | "RIGHT";
  line: number;
};

/** `#pull-v2-file-<hash>` anchor id for a diff file block. */
export function fileAnchorId(path: string): string {
  let h = 5381;
  for (let i = 0; i < path.length; i++) {
    h = ((h << 5) + h + path.charCodeAt(i)) | 0;
  }
  return `pull-v2-file-${(h >>> 0).toString(36)}`;
}

export function pathBasename(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? path : path.slice(i + 1);
}

export function pathDirname(path: string): string {
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
