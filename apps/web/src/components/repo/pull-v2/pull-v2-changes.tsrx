import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { PullCommentPublic, PullDiffFile } from "@oxidean/api-client";
import {
  Check,
  GitCommitHorizontal,
  MessageSquare,
  PanelLeft,
  PanelLeftClose,
  Settings2,
} from "@octanejs/lucide";
import { useEffect, useMemo, useRef, useState } from "octane";
import { AppLink } from "@/components/ui/app-link";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { apiClient } from "@/lib/api-client";
import { DIFF_FILES_PAGE_SIZE } from "@/lib/diff-budget";
import { useIncrementalList } from "@/lib/incremental-list.tsrx";
import { cn } from "@/lib/utils";
import { fileAnchorId, type DraftTarget, type PendingLineComment } from "./pull-v2-diff-utils";
import { PullV2CommentsPanel } from "./pull-v2-comments-panel";
import { PullV2DiffFile } from "./pull-v2-diff-file";
import { PullV2FileTree } from "./pull-v2-file-tree";
import { PullV2ReviewBar } from "./pull-v2-review-bar";

type PullV2ChangesProps = {
  owner: string;
  repoName: string;
  number: number;
  canInteract: boolean;
  isAuthor: boolean;
  currentUserId: string;
  /** `/pull/N/changes/<sha>` — scoped-commit notice (S9). */
  changesSha?: string;
  /** PR head sha — anchors new review comments (S6 commit_sha field). */
  headSha?: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function scrollToFile(path: string) {
  const el = document.getElementById(fileAnchorId(path));
  if (el && typeof el.scrollIntoView === "function") {
    el.scrollIntoView({ block: "start", behavior: "smooth" });
  }
}

/**
 * Files-changed tab of the new PR experience (WI-0001 S4–S7): collapsible
 * file tree, filter/expand/settings toolbar, per-file viewed state, any-line
 * "+ comment" affordances feeding a local pending-review list, a comments
 * side panel, and the sticky submit-review bar.
 */
export function PullV2Changes(props: PullV2ChangesProps) @{
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState("");
  const [view, setView] = useState<"unified" | "split">("unified");
  const [wrap, setWrap] = useState(false);
  const [compact, setCompact] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [panelOpen, setPanelOpen] = useState(false);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [viewedList, setViewedList] = useState<string[]>([]);
  const [pending, setPending] = useState<PendingLineComment[]>([]);
  const [draft, setDraft] = useState<DraftTarget | null>(null);
  const [draftBody, setDraftBody] = useState("");
  const [reviewBusy, setReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  const seqRef = useRef(0);

  const filesQ = useQuery({
    queryKey: ["pull", "files", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.files({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const viewedQ = useQuery({
    queryKey: ["pull", "files", "viewed", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.filesViewed.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    retry: false,
  });

  const commentsQ = useQuery({
    queryKey: ["pull", "comments", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.comments.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const files: PullDiffFile[] = filesQ.data?.files ?? [];
  const comments: PullCommentPublic[] = commentsQ.data?.comments ?? [];

  // S5 — hydrate local viewed state once the server list lands.
  useEffect(() => {
    if (viewedQ.data) setViewedList(viewedQ.data.viewed_paths ?? []);
  }, [viewedQ.data]);

  const viewedSet = useMemo(() => new Set(viewedList), [viewedList]);

  const needle = filter.trim().toLowerCase();
  const filtered = useMemo(
    () => (needle ? files.filter((f) => f.path.toLowerCase().includes(needle)) : files),
    [files, needle],
  );

  const commentCounts = useMemo(() => {
    const m = new Map<string, number>();
    for (const c of comments) {
      if (!c.path) continue;
      m.set(c.path, (m.get(c.path) ?? 0) + 1);
    }
    for (const p of pending) {
      m.set(p.path, (m.get(p.path) ?? 0) + 1);
    }
    return m;
  }, [comments, pending]);

  const viewedInFiles = files.filter((f) => viewedSet.has(f.path)).length;

  const list = useIncrementalList(
    filtered.length,
    DIFF_FILES_PAGE_SIZE,
    `${props.owner}/${props.repoName}#${props.number}|${needle}`,
  );
  const shown = filtered.slice(0, list.count);

  function toggleDirFile(path: string) {
    setCollapsed((prev) => ({ ...prev, [path]: !(prev[path] ?? false) }));
  }

  function setAllCollapsed(next: boolean) {
    if (!next) {
      setCollapsed({});
      return;
    }
    const m: Record<string, boolean> = {};
    for (const f of files) m[f.path] = true;
    setCollapsed(m);
  }

  async function toggleViewed(path: string, viewed: boolean) {
    const prev = viewedList;
    setViewedList(viewed ? [...prev, path] : prev.filter((p) => p !== path));
    const res = await apiClient.pull.filesViewed.set({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      path,
      viewed,
    });
    if (res.ok) {
      setViewedList(res.data.viewed_paths ?? []);
    } else {
      setViewedList(prev);
    }
  }

  function startDraft(target: DraftTarget) {
    setDraft(target);
    setDraftBody("");
  }

  function submitDraft() {
    const t = draft;
    const body = draftBody.trim();
    if (!t || !body) return;
    seqRef.current += 1;
    setPending((prev) => [...prev, { id: seqRef.current, ...t, body }]);
    setDraft(null);
    setDraftBody("");
  }

  function discardPending(id: number) {
    setPending((prev) => prev.filter((c) => c.id !== id));
  }

  async function submitReview(state: "commented" | "approved" | "changes_requested", body: string) {
    setReviewBusy(true);
    setReviewError(null);
    const res = await apiClient.pull.reviews.submit({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      state,
      body: body.trim() ? body : null,
    });
    if (!res.ok) {
      setReviewBusy(false);
      setReviewError(res.error.message || "Review failed");
      return;
    }
    // Pending line comments post after the review — sequential so a failure
    // keeps the remaining list intact for a retry (S6). Anchors land on the
    // viewed commit: the scoped `<sha>` when the URL carries one, else head.
    const anchorSha = props.changesSha ?? props.headSha ?? null;
    for (const c of pending) {
      const cr = await apiClient.pull.comments.create({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        body: c.body,
        path: c.path,
        side: c.side,
        line: c.line,
        commit_sha: anchorSha,
      });
      if (!cr.ok) {
        setReviewBusy(false);
        setReviewError(cr.error.message || "Could not post a pending comment");
        return;
      }
    }
    setPending([]);
    setDraft(null);
    setDraftBody("");
    setReviewBusy(false);
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: ["pull", "comments", props.owner, props.repoName, props.number],
      }),
      queryClient.invalidateQueries({
        queryKey: ["pull", "reviews", props.owner, props.repoName, props.number],
      }),
    ]);
  }

  const base = `/${props.owner}/${props.repoName}/pull/${props.number}`;

  <div className="space-y-3" data-testid="pull-v2-changes">
    @if (props.changesSha) {
      <div
        className="flex flex-wrap items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 text-[13px] text-foreground"
        data-testid="pull-v2-changes-sha-notice"
      >
        <GitCommitHorizontal aria-hidden size={14} className="text-primary" />
        <span>
          Viewing changes from commit{" "}
          <span className="font-mono">{(props.changesSha ?? "").slice(0, 7) as string}</span>
        </span>
        <AppLink
          href={`${base}/changes` as string}
          className="font-medium text-primary no-underline hover:underline"
          data-testid="pull-v2-changes-show-all"
        >
          Show all changes
        </AppLink>
      </div>;
    }

    <div className="flex items-start gap-4">
      @if (sidebarOpen) {
        <aside className="sticky top-36 hidden max-h-[calc(100vh-10rem)] w-[260px] shrink-0 self-start overflow-y-auto rounded-md border border-border bg-card md:block">
          <PullV2FileTree
            files={filtered}
            viewed={viewedSet}
            commentCounts={commentCounts}
            onToggleViewed={(path, v) => void toggleViewed(path, v)}
          />
        </aside>;
      }

      <div className="min-w-0 flex-1 space-y-3">
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-border bg-muted/30 px-2 py-1.5">
          <button
            type="button"
            className="inline-flex size-7 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
            aria-label="Toggle file tree"
            aria-expanded={sidebarOpen}
            data-testid="pull-v2-tree-toggle"
            onClick={() => setSidebarOpen(!sidebarOpen)}
          >
            @if (sidebarOpen) {
              <PanelLeftClose aria-hidden size={15} />;
            } @else {
              <PanelLeft aria-hidden size={15} />;
            }
          </button>
          <input
            type="search"
            className="min-w-[10rem] flex-1 rounded-md border border-border bg-background px-2 py-1 text-[13px] outline-none focus-visible:border-ring"
            placeholder="Filter files"
            value={filter}
            data-testid="pull-v2-file-filter"
            onInput={(e) => setFilter((e.currentTarget as HTMLInputElement).value)}
          />
          @if (needle) {
            <span
              className="text-[12px] text-muted-foreground"
              data-testid="pull-v2-filter-count"
            >
              {`${filtered.length} of ${files.length}` as string}
            </span>;
          }
          <span
            className="ml-auto text-[12px] text-muted-foreground"
            data-testid="pull-v2-viewed-progress"
          >
            {`${viewedInFiles} of ${files.length} files viewed` as string}
          </span>
          <button
            type="button"
            className="rounded-sm px-1.5 py-1 text-[12px] text-muted-foreground hover:bg-muted hover:text-foreground"
            data-testid="pull-v2-expand-all"
            onClick={() => setAllCollapsed(false)}
          >
            Expand all
          </button>
          <button
            type="button"
            className="rounded-sm px-1.5 py-1 text-[12px] text-muted-foreground hover:bg-muted hover:text-foreground"
            data-testid="pull-v2-collapse-all"
            onClick={() => setAllCollapsed(true)}
          >
            Collapse all
          </button>
          <button
            type="button"
            className={
              cn(
                "inline-flex size-7 items-center justify-center rounded-sm",
                panelOpen
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              ) as string
            }
            aria-label="Toggle comments panel"
            aria-expanded={panelOpen}
            data-testid="pull-v2-comments-toggle"
            onClick={() => setPanelOpen(!panelOpen)}
          >
            <MessageSquare aria-hidden size={15} />
          </button>
          <DropdownMenu>
            <DropdownMenuTrigger
              className="inline-flex size-7 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
              aria-label="Diff settings"
              data-testid="pull-v2-diff-settings"
            >
              <Settings2 aria-hidden size={15} />
            </DropdownMenuTrigger>
            <DropdownMenuContent className="min-w-48">
              <DropdownMenuItem
                data-testid="pull-v2-view-unified"
                onClick={() => setView("unified")}
              >
                <Check aria-hidden size={13} className={view === "unified" ? "" : "invisible"} />
                Unified view
              </DropdownMenuItem>
              <DropdownMenuItem data-testid="pull-v2-view-split" onClick={() => setView("split")}>
                <Check aria-hidden size={13} className={view === "split" ? "" : "invisible"} />
                Split view
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuCheckboxItem
                checked={wrap}
                onCheckedChange={(v) => setWrap(v)}
                data-testid="pull-v2-wrap-lines"
              >
                Wrap lines
              </DropdownMenuCheckboxItem>
              <DropdownMenuCheckboxItem
                checked={compact}
                onCheckedChange={(v) => setCompact(v)}
                data-testid="pull-v2-compact-density"
              >
                Compact density
              </DropdownMenuCheckboxItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        @if (filesQ.isPending) {
          <p className="text-sm text-muted-foreground">Loading files…</p>;
        } @else {
          @if (filesQ.isError) {
            <p role="alert" className="text-sm text-destructive">
              Could not load diff.
            </p>;
          } @else {
            @if (files.length === 0) {
              <p className="text-sm text-muted-foreground">No file changes.</p>;
            } @else {
              <>
                @if (filtered.length === 0) {
                  <p className="text-sm text-muted-foreground">
                    {`No files match "${filter.trim()}".` as string}
                  </p>;
                }
                <div className="space-y-4">
                  @for (const f of shown; key f.path) {
                    <PullV2DiffFile
                      file={f}
                      view={view}
                      wrap={wrap}
                      compact={compact}
                      expanded={!(collapsed[f.path] ?? false)}
                      viewed={viewedSet.has(f.path)}
                      canInteract={props.canInteract}
                      pending={pending.filter((c) => c.path === f.path)}
                      submitted={comments.filter((c) => c.path === f.path)}
                      draft={draft?.path === f.path ? draft : null}
                      draftBody={draftBody}
                      onToggleExpand={() => toggleDirFile(f.path)}
                      onToggleViewed={(v) => void toggleViewed(f.path, v)}
                      onDraftBody={setDraftBody}
                      onStartDraft={startDraft}
                      onSubmitDraft={submitDraft}
                      onCancelDraft={() => {
                        setDraft(null);
                        setDraftBody("");
                      }}
                      onDiscardPending={discardPending}
                    />;
                  }
                </div>
                @if (list.remaining > 0) {
                  <div ref={list.sentinelRef} className="flex justify-center py-2">
                    <button
                      type="button"
                      className="rounded-md border border-border px-3 py-1.5 text-[14px] text-foreground hover:bg-muted"
                      onClick={() => list.showMore()}
                    >
                      {`Load more files (${list.remaining} remaining)` as string}
                    </button>
                  </div>;
                }
              </>;
            }
          }
        }
      </div>

      @if (panelOpen) {
        <aside className="sticky top-36 hidden max-h-[calc(100vh-10rem)] w-80 shrink-0 self-start overflow-hidden rounded-md border border-border bg-card lg:block">
          <PullV2CommentsPanel
            comments={comments}
            pendingCount={pending.length}
            currentUserId={props.currentUserId}
            onClose={() => setPanelOpen(false)}
            onJump={scrollToFile}
          />
        </aside>;
      }
    </div>

    @if (props.canInteract) {
      <div className="sticky bottom-0 z-10">
        <PullV2ReviewBar
          pendingCount={pending.length}
          isAuthor={props.isAuthor}
          busy={reviewBusy}
          error={reviewError}
          onSubmit={(state, body) => void submitReview(state, body)}
        />
      </div>;
    }
  </div>;
}
