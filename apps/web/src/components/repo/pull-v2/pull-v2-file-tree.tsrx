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
  ListFilter,
  MessageSquare,
} from "@octanejs/lucide";
import { useMemo, useState } from "octane";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
  /** Viewed checkboxes allowed — hidden on scoped (sha/range/since) diffs. */
  showViewed: boolean;
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
  // Tree-local filter (filename substring + status toggles) — narrows tree
  // rows only; the diff column still follows the toolbar filter.
  const [nameFilter, setNameFilter] = useState("");
  const [hiddenStatuses, setHiddenStatuses] = useState<ReadonlySet<string>>(() => new Set());
  const statuses = useMemo(() => {
    const s = new Set<string>();
    for (const f of props.files) s.add(f.status || "modified");
    return [...s].sort();
  }, [props.files]);
  const nameNeedle = nameFilter.trim().toLowerCase();
  const treeFiles = useMemo(
    () =>
      props.files.filter((f) => {
        if (hiddenStatuses.has(f.status || "modified")) return false;
        return !nameNeedle || f.path.toLowerCase().includes(nameNeedle);
      }),
    [props.files, hiddenStatuses, nameNeedle],
  );
  const tree = useMemo(() => buildFileTree(treeFiles), [treeFiles]);
  const stats = useMemo(
    () =>
      new Map<string, readonly [number, number]>(
        props.files.map((f) => [f.path, diffPatchStatCounts(f.patch)] as const),
      ),
    [props.files],
  );
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const rows = flattenFileTree(tree, collapsed);
  const filterActive = nameNeedle.length > 0 || hiddenStatuses.size > 0;

  function toggleStatus(status: string, checked: boolean) {
    setHiddenStatuses((prev) => {
      const next = new Set(prev);
      if (checked) next.delete(status);
      else next.add(status);
      return next;
    });
  }

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
    <div className="flex items-center gap-1 px-2 py-0.5">
      <span className="min-w-0 flex-1 truncate text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        Files
      </span>
      <DropdownMenu>
        <DropdownMenuTrigger
          className={
            cn(
              "inline-flex size-6 items-center justify-center rounded-sm",
              filterActive
                ? "bg-primary/10 text-primary"
                : "text-muted-foreground hover:bg-muted hover:text-foreground",
            ) as string
          }
          aria-label="Filter file tree"
          data-testid="pull-v2-tree-filter"
        >
          <ListFilter aria-hidden size={13} />
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-56" align="start">
          <div className="px-2 py-1.5">
            <input
              type="search"
              className="w-full rounded-md border border-border bg-background px-2 py-1 text-[12px] outline-none focus-visible:border-ring"
              placeholder="Filter by filename"
              value={nameFilter}
              data-testid="pull-v2-tree-filter-name"
              onInput={(e) => setNameFilter((e.currentTarget as HTMLInputElement).value)}
            />
          </div>
          <DropdownMenuSeparator />
          @for (const s of statuses; key s) {
            <DropdownMenuCheckboxItem
              checked={!hiddenStatuses.has(s)}
              onCheckedChange={(v) => toggleStatus(s, v)}
              data-testid={`pull-v2-tree-filter-status-${s}`}
            >
              {s as string}
            </DropdownMenuCheckboxItem>;
          }
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
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
          @if (props.canInteract && props.showViewed) {
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
