import type { PullDiffFile } from "@oxidean/api-client";
import {
  Check,
  ChevronDown,
  ChevronRight,
  FileCode,
  FilePen,
  FilePlus2,
  FileX,
  Folder,
  MessageSquare,
} from "@octanejs/lucide";
import { useMemo, useState } from "octane";
import { diffPatchStatCounts } from "@/lib/parse-unified-diff";
import { cn } from "@/lib/utils";
import { buildFileTree, fileAnchorId, fileStatusMeta, flattenFileTree } from "./pull-v2-diff-utils";

type PullV2FileTreeProps = {
  /** Files visible after the toolbar filter — the tree mirrors the diff list. */
  files: PullDiffFile[];
  viewed: ReadonlySet<string>;
  /** Submitted + pending comment count per path. */
  commentCounts: ReadonlyMap<string, number>;
  /** Signed-in viewers only — viewed marks require a verified session. */
  canInteract: boolean;
  onToggleViewed: (path: string, viewed: boolean) => void;
};

function FileStatusIcon(props: { status: string }) @{
  const meta = fileStatusMeta(props.status);
  <span className={cn("inline-flex shrink-0", meta.className as string)} title={props.status}>
    @if (props.status === "added") {
      <FilePlus2 aria-hidden size={13} />;
    } @else {
      @if (props.status === "deleted" || props.status === "removed") {
        <FileX aria-hidden size={13} />;
      } @else {
        @if (props.status === "renamed") {
          <FileCode aria-hidden size={13} />;
        } @else {
          <FilePen aria-hidden size={13} />;
        }
      }
    }
  </span>;
}

/**
 * GitHub-style collapsible file tree (WI-0001 S4): directories collapse
 * single-child chains, file rows show status + +a/−d + comment counts and
 * double as jump links to the diff block. The trailing toggle is the
 * per-file "viewed" checkbox (S5).
 */
export function PullV2FileTree(props: PullV2FileTreeProps) @{
  const tree = useMemo(() => buildFileTree(props.files), [props.files]);
  const stats = useMemo(
    () =>
      new Map<string, readonly [number, number]>(
        props.files.map((f) => [f.path, diffPatchStatCounts(f.patch)] as const),
      ),
    [props.files],
  );
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const rows = flattenFileTree(tree, collapsed);

  function toggleDir(path: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  function jump(path: string) {
    const el = document.getElementById(fileAnchorId(path));
    if (el && typeof el.scrollIntoView === "function") {
      el.scrollIntoView({ block: "start", behavior: "smooth" });
    }
  }

  <nav className="py-1" aria-label="Changed files" data-testid="pull-v2-file-tree">
    @for (const row of rows; key row.key) {
      @if (row.kind === "dir") {
        <button
          type="button"
          className="flex w-full items-center gap-1 px-2 py-[3px] text-left text-[13px] text-foreground hover:bg-muted/60"
          style={{ paddingLeft: `${row.depth * 12 + 8}px` }}
          aria-expanded={!row.collapsed}
          onClick={() => toggleDir(row.path)}
        >
          @if (row.collapsed) {
            <ChevronRight
              aria-hidden
              size={12}
              className="shrink-0 text-muted-foreground"
            />;
          } @else {
            <ChevronDown
              aria-hidden
              size={12}
              className="shrink-0 text-muted-foreground"
            />;
          }
          <Folder aria-hidden size={13} className="shrink-0 text-muted-foreground" />
          <span className="min-w-0 truncate">{row.label as string}</span>
        </button>;
      } @else {
        <div className="group flex items-center gap-1 pr-2" data-testid="pull-v2-tree-file">
          {/* raw-anchor-ok — same-page hash jump to the diff block */}
          <a
            href={`#${fileAnchorId(row.path)}` as string}
            className={cn(
              "flex min-w-0 flex-1 items-center gap-1.5 py-[3px] text-[13px] no-underline",
              props.viewed.has(row.path) ? "text-muted-foreground" : "text-foreground",
            )}
            style={{ paddingLeft: `${row.depth * 12 + 24}px` }}
            onClick={(e) => {
              e.preventDefault();
              jump(row.path);
            }}
          >
            <FileStatusIcon status={row.file?.status ?? ""} />
            <span className="min-w-0 flex-1 truncate">{row.label as string}</span>
            @if ((props.commentCounts.get(row.path) ?? 0) > 0) {
              <span
                className="inline-flex shrink-0 items-center gap-0.5 text-[11px] text-muted-foreground"
                title={`${props.commentCounts.get(row.path) ?? 0} comments`}
              >
                <MessageSquare aria-hidden size={10} />
                {(props.commentCounts.get(row.path) ?? 0) as number}
              </span>;
            }
            @if (
              (stats.get(row.path)?.[0] ?? 0) + (stats.get(row.path)?.[1] ?? 0) >
              0
            ) {
              <span className="shrink-0 tabular-nums text-[11px]">
                <span className="text-success">
                  {`+${stats.get(row.path)?.[0] ?? 0}` as string}
                </span>{" "}
                <span className="text-destructive">
                  {`−${stats.get(row.path)?.[1] ?? 0}` as string}
                </span>
              </span>;
            }
          </a>
          @if (props.canInteract) {
            <button
              type="button"
              role="checkbox"
              aria-checked={props.viewed.has(row.path)}
              className={cn(
                "inline-flex size-4 shrink-0 items-center justify-center rounded-sm border",
                props.viewed.has(row.path)
                  ? "border-primary bg-primary text-primary-foreground"
                  : "border-input bg-card text-transparent hover:border-muted-foreground",
              )}
              title={
                props.viewed.has(row.path) ? "Mark as not viewed" : "Mark as viewed"
              }
              data-testid="pull-v2-tree-viewed"
              onClick={() =>
                props.onToggleViewed(row.path, !props.viewed.has(row.path))
              }
            >
              <Check aria-hidden size={11} />
            </button>;
          }
        </div>;
      }
    }
  </nav>;
}
