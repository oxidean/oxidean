import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  PullCommentPublic,
  PullCommentsListResponse,
  PullDiffFile,
  PullFilesRequest,
} from "@oxidean/api-client";
import {
  ArrowLeft,
  Check,
  ChevronDown,
  GitCommitHorizontal,
  GitPullRequest,
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
  DropdownMenuLinkItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { apiClient } from "@/lib/api-client";
import { DIFF_FILES_PAGE_SIZE } from "@/lib/diff-budget";
import { useIncrementalList } from "@/lib/incremental-list.tsrx";
import { navigate } from "@/lib/nav";
import { blobHref } from "@/lib/repo-browse";
import { cn } from "@/lib/utils";
import {
  changesScopeKey,
  parseChangesScope,
  scopeAnchorSha,
  type ChangesScope,
  type DraftTarget,
} from "./pull-v2-diff-utils";
import { PullV2DiffFile } from "./pull-v2-diff-file";
import { PullV2FileTree } from "./pull-v2-file-tree";
import { PullV2ReviewBar } from "./pull-v2-review-bar";

type PullV2ChangesProps = {
  owner: string;
  repoName: string;
  number: number;
  canInteract: boolean;
  /** Write+ capability — pending drafts + review submit are Write-gated. */
  canWrite?: boolean;
  isAuthor: boolean;
  currentUserId: string;
  /** `/pull/N/changes/<sha>` or `/pull/N/changes/<base>..<head>` (#261). */
  changesSha?: string;
  /** PR head sha — anchors new review comments + "View file" links. */
  headSha?: string;
  /** Lifted comments-panel state — the page header toggle drives it too. */
  panelOpen?: boolean;
  onPanelToggle?: (open: boolean) => void;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** File-tree column bounds (px) — the draggable splitter clamps to these. */
const TREE_MIN_W = 200;
const TREE_MAX_W = 480;
const TREE_DEFAULT_W = 260;

type FilesResult = {
  files: PullDiffFile[];
  empty: boolean;
  /** `since_review` fell back to the unscoped diff (no submitted review). */
  noPriorReview: boolean;
};

/**
 * Files-changed tab of the new PR experience (WI-0001 S4–S7, #261–#263):
 * collapsible file tree (filterable, resizable), scope dropdown + notices for
 * single-commit / range / since-review diffs, per-file viewed state on the
 * unscoped view only, "+ comment" + gutter-drag line anchors writing
 * server-side pending drafts, a comments side panel, and the sticky
 * submit-review bar.
 */
export function PullV2Changes(props: PullV2ChangesProps) @{
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState("");
  const [view, setView] = useState<"unified" | "split">("unified");
  const [wrap, setWrap] = useState(false);
  const [compact, setCompact] = useState(false);
  const [hideWhitespace, setHideWhitespace] = useState(false);
  const [contextLines, setContextLines] = useState<number | null>(null);
  const [minimizeComments, setMinimizeComments] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [panelLocal, setPanelLocal] = useState(false);
  const panelOpen = () => props.panelOpen ?? panelLocal;
  const setPanelOpen = (v: boolean) =>
    props.onPanelToggle ? props.onPanelToggle(v) : setPanelLocal(v);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [viewedList, setViewedList] = useState<string[]>([]);
  const [draft, setDraft] = useState<DraftTarget | null>(null);
  const [draftBody, setDraftBody] = useState("");
  const [draftBusy, setDraftBusy] = useState(false);
  const [draftError, setDraftError] = useState<string | null>(null);
  const [reviewBusy, setReviewBusy] = useState(false);
  const [reviewError, setReviewError] = useState<string | null>(null);
  // `since` is a client-only scope (no URL) — picking it from the scope menu
  // overlays whatever `changesSha` carries; URL changes clear it.
  const [sinceActive, setSinceActive] = useState(false);
  const [rangeMode, setRangeMode] = useState(false);
  const [rangeStart, setRangeStart] = useState("");
  const [rangeEnd, setRangeEnd] = useState("");
  const treeWidthKey = `oxidean:pull-v2-tree-width:${props.owner}/${props.repoName}`;
  const [treeWidth, setTreeWidth] = useState(() => {
    if (typeof localStorage === "undefined") return TREE_DEFAULT_W;
    const v = Number(localStorage.getItem(treeWidthKey));
    return Number.isFinite(v) && v > 0
      ? Math.min(TREE_MAX_W, Math.max(TREE_MIN_W, v))
      : TREE_DEFAULT_W;
  });
  const treeDragRef = useRef<{ x: number; w: number; next: number } | null>(null);

  useEffect(() => {
    setSinceActive(false);
    setRangeMode(false);
  }, [props.changesSha]);

  const scope: ChangesScope = sinceActive ? { kind: "since" } : parseChangesScope(props.changesSha);
  const scopeKey = changesScopeKey(scope);

  const filesQ = useQuery({
    queryKey: [
      "pull",
      "files",
      props.owner,
      props.repoName,
      props.number,
      scopeKey,
      hideWhitespace,
      contextLines,
    ] as const,
    queryFn: async (): Promise<FilesResult> => {
      const baseInput: PullFilesRequest = {
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        w: hideWhitespace || undefined,
        context: contextLines ?? undefined,
      };
      const input: PullFilesRequest = { ...baseInput };
      if (scope.kind === "sha") input.sha = scope.sha;
      else if (scope.kind === "range") {
        input.base_sha = scope.base;
        input.head_sha = scope.head;
      } else if (scope.kind === "since") input.since_review = true;
      let res = await apiClient.pull.files(input);
      let noPriorReview = false;
      // "Changes since your last review" with no submitted review → the
      // unscoped diff plus the fallback notice (GitHub parity).
      if (!res.ok && res.error.code === "pull.no_prior_review") {
        res = await apiClient.pull.files(baseInput);
        noPriorReview = true;
      }
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return { ...res.data, noPriorReview };
    },
  });

  const commitsQ = useQuery({
    queryKey: ["pull", "commits", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.commits({
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
    enabled: props.canInteract,
    retry: false,
  });

  const commentsKey = ["pull", "comments", props.owner, props.repoName, props.number] as const;
  const commentsQ = useQuery({
    queryKey: commentsKey,
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
  const noPriorReview = filesQ.data?.noPriorReview ?? false;
  const allComments: PullCommentPublic[] = commentsQ.data?.comments ?? [];
  const pendingComments = useMemo(() => allComments.filter((c) => c.pending), [allComments]);
  const submittedComments = useMemo(() => allComments.filter((c) => !c.pending), [allComments]);

  // Any scoped view (sha / range / since-with-review) drops viewed marks.
  const scopedView = scope.kind !== "none" && !noPriorReview;
  const marksEnabled = props.canInteract && !scopedView;
  const anchorSha = scopeAnchorSha(scope, props.headSha);

  // S5 — hydrate local viewed state once the server list lands (first
  // response only — a later refetch must not clobber a fresh toggle).
  const viewedHydrated = useRef(false);
  useEffect(() => {
    if (viewedQ.data && !viewedHydrated.current) {
      viewedHydrated.current = true;
      setViewedList(viewedQ.data.viewed_paths ?? []);
    }
  }, [viewedQ.data]);

  const viewedSet = useMemo(() => new Set(viewedList), [viewedList]);

  const needle = filter.trim().toLowerCase();
  const filtered = useMemo(
    () => (needle ? files.filter((f) => f.path.toLowerCase().includes(needle)) : files),
    [files, needle],
  );

  const commentCounts = useMemo(() => {
    const m = new Map<string, number>();
    for (const c of allComments) {
      if (!c.path) continue;
      m.set(c.path, (m.get(c.path) ?? 0) + 1);
    }
    return m;
  }, [allComments]);

  const viewedInFiles = files.filter((f) => viewedSet.has(f.path)).length;

  const list = useIncrementalList(
    filtered.length,
    DIFF_FILES_PAGE_SIZE,
    `${props.owner}/${props.repoName}#${props.number}|${scopeKey}|${needle}`,
  );
  const shown = filtered.slice(0, list.count);
  const commits = commitsQ.data?.commits ?? [];

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
    if (!marksEnabled) return;
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

  function invalidateComments() {
    return queryClient.invalidateQueries({ queryKey: commentsKey });
  }

  function startDraft(target: DraftTarget) {
    setDraft(target);
    setDraftBody("");
    setDraftError(null);
  }

  // `pending: true` keeps a review draft server-side; `false` posts the
  // comment immediately (GitHub's single-comment path).
  async function submitDraft(pending: boolean) {
    const t = draft;
    const body = draftBody.trim();
    if (!t || !body || draftBusy) return;
    setDraftBusy(true);
    setDraftError(null);
    const res = await apiClient.pull.comments.create({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      body,
      path: t.path,
      side: t.side,
      line: t.line,
      start_line: t.startLine ?? null,
      commit_sha: anchorSha,
      pending,
    });
    setDraftBusy(false);
    if (!res.ok) {
      setDraftError(res.error.message || "Could not save comment");
      return;
    }
    setDraft(null);
    setDraftBody("");
    await invalidateComments();
  }

  async function discardPending(id: string) {
    const res = await apiClient.pull.comments.delete({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      commentId: id,
    });
    if (res.ok) {
      await invalidateComments();
    } else {
      setReviewError(res.error.message || "Could not discard comment");
    }
  }

  async function submitReview(state: "commented" | "approved" | "changes_requested", body: string) {
    setReviewBusy(true);
    setReviewError(null);
    // Pending drafts already exist server-side — submit links their ids.
    // Read them fresh from the cache at click time so a comment saved or
    // discarded mid-flight can't ride a stale closure.
    const fresh = queryClient.getQueryData<PullCommentsListResponse>(commentsKey);
    const commentIds = (fresh?.comments ?? allComments).filter((c) => c.pending).map((c) => c.id);
    const res = await apiClient.pull.reviews.submit({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      state,
      body: body.trim() ? body : null,
      comment_ids: commentIds,
    });
    if (!res.ok) {
      setReviewBusy(false);
      setReviewError(res.error.message || "Review failed");
      return;
    }
    setDraft(null);
    setDraftBody("");
    setReviewBusy(false);
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: commentsKey }),
      queryClient.invalidateQueries({
        queryKey: ["pull", "reviews", props.owner, props.repoName, props.number],
      }),
    ]);
  }

  function onSplitterDown(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    // Synthetic/dispatched pointer ids have no active pointer — capture throws
    // NotFoundError in Chromium; the drag still works without it.
    try {
      el.setPointerCapture?.(e.pointerId);
    } catch {
      /* no active pointer for this id */
    }
    treeDragRef.current = { x: e.clientX, w: treeWidth, next: treeWidth };
  }

  function onSplitterMove(e: PointerEvent) {
    const o = treeDragRef.current;
    if (!o) return;
    const next = Math.min(TREE_MAX_W, Math.max(TREE_MIN_W, Math.round(o.w + (e.clientX - o.x))));
    // Carry the latest width in the drag ref — a pointerup landing in the
    // same event burst as the last move would otherwise persist a stale
    // render-time `treeWidth` before the state flush.
    o.next = next;
    setTreeWidth(next);
  }

  function onSplitterUp(e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    try {
      if (el.hasPointerCapture?.(e.pointerId)) el.releasePointerCapture(e.pointerId);
    } catch {
      /* no active pointer for this id */
    }
    const o = treeDragRef.current;
    if (o) {
      treeDragRef.current = null;
      try {
        localStorage.setItem(treeWidthKey, String(o.next));
      } catch {
        /* storage unavailable — width stays session-local */
      }
    }
  }

  function pickRangeStart(sha: string) {
    setRangeStart(sha);
    if (sha && rangeEnd && sha !== rangeEnd) {
      navigate(`${base}/changes/${sha}..${rangeEnd}`);
    }
  }

  function pickRangeEnd(sha: string) {
    setRangeEnd(sha);
    if (sha && rangeStart && sha !== rangeStart) {
      navigate(`${base}/changes/${rangeStart}..${sha}`);
    }
  }

  const base = `/${props.owner}/${props.repoName}/pull/${props.number}`;
  const scopeLabel =
    scope.kind === "sha"
      ? scope.sha.slice(0, 7)
      : scope.kind === "range"
        ? `${scope.base.slice(0, 7)}..${scope.head.slice(0, 7)}`
        : scope.kind === "since"
          ? "Since your last review"
          : "All commits";
  const viewFileRef = props.headSha ?? "";

  <div className="space-y-3" data-testid="pull-v2-changes">
    @if (noPriorReview) {
      <div
        className="flex flex-wrap items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 text-[13px] text-foreground"
        data-testid="pull-v2-changes-no-prior-notice"
      >
        <GitPullRequest aria-hidden size={14} className="text-primary" />
        <span>No submitted review yet — showing all changes</span>
        <AppLink
          href={`${base}/changes` as string}
          className="font-medium text-primary no-underline hover:underline"
          data-testid="pull-v2-changes-show-all"
          onClick={() => setSinceActive(false)}
        >
          Show all changes
        </AppLink>
      </div>;
    } @else {
      @if (
        scope.kind === "sha" ||
        scope.kind === "range" ||
        scope.kind === "since"
      ) {
        <div
          className="flex flex-wrap items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 text-[13px] text-foreground"
          data-testid={
            scope.kind === "sha"
              ? "pull-v2-changes-sha-notice"
              : scope.kind === "range"
                ? "pull-v2-changes-range-notice"
                : "pull-v2-changes-since-notice"
          }
        >
          <GitCommitHorizontal aria-hidden size={14} className="text-primary" />
          @if (scope.kind === "sha") {
            <span>
              Viewing changes from commit{" "}
              <span className="font-mono">{scope.sha.slice(0, 7) as string}</span>
            </span>;
          } @else {
            @if (scope.kind === "range") {
              <span>
                Viewing changes in range{" "}
                <span className="font-mono">
                  {`${scope.base.slice(0, 7)}..${scope.head.slice(0, 7)}` as string}
                </span>
              </span>;
            } @else {
              <span>Changes since your last review</span>;
            }
          }
          <AppLink
            href={`${base}/changes` as string}
            className="font-medium text-primary no-underline hover:underline"
            data-testid="pull-v2-changes-show-all"
          >
            Show all changes
          </AppLink>
        </div>;
      }
    }

    <div className="flex items-start gap-4">
      @if (sidebarOpen) {
        <>
          <aside
            className="sticky top-36 hidden max-h-[calc(100vh-10rem)] shrink-0 self-start overflow-y-auto rounded-md border border-border bg-card md:block"
            style={{ width: `${treeWidth}px` }}
          >
            <PullV2FileTree
              files={filtered}
              viewed={viewedSet}
              commentCounts={commentCounts}
              canInteract={props.canInteract}
              showViewed={marksEnabled}
              onToggleViewed={(path, v) => void toggleViewed(path, v)}
            />
          </aside>
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="Resize file tree"
            className="sticky top-36 hidden h-[calc(100vh-10rem)] w-1.5 shrink-0 cursor-col-resize touch-none self-start rounded-full hover:bg-muted-foreground/30 md:block"
            data-testid="pull-v2-tree-splitter"
            onPointerDown={onSplitterDown}
            onPointerMove={onSplitterMove}
            onPointerUp={onSplitterUp}
            onPointerCancel={onSplitterUp}
          />
        </>;
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
          <DropdownMenu
            onOpenChange={(open) => {
              if (!open) {
                setRangeMode(false);
                setRangeStart("");
                setRangeEnd("");
              }
            }}
          >
            <DropdownMenuTrigger
              className="inline-flex h-7 items-center gap-1 rounded-md border border-border bg-background px-2 text-[12px] font-medium text-foreground hover:bg-muted"
              aria-label="Select commits to compare"
              data-testid="pull-v2-scope-menu"
            >
              <span className="max-w-40 truncate">{scopeLabel as string}</span>
              <ChevronDown aria-hidden size={12} className="text-muted-foreground" />
            </DropdownMenuTrigger>
            <DropdownMenuContent className="max-h-[70vh] min-w-72 overflow-y-auto" align="start">
              @if (rangeMode) {
                <>
                  <DropdownMenuItem
                    closeOnClick={false}
                    data-testid="pull-v2-scope-range-back"
                    onClick={() => setRangeMode(false)}
                  >
                    <ArrowLeft aria-hidden size={13} />
                    Back
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <div className="space-y-2 px-2 py-1.5">
                    <label className="block text-[12px] font-medium text-foreground">
                      Start commit
                      <select
                        className="mt-1 w-full rounded-md border border-border bg-background px-1.5 py-1 text-[12px]"
                        value={rangeStart}
                        suppressNativeChangeWarning
                        data-testid="pull-v2-range-start"
                        onChange={(e) =>
                          pickRangeStart((e.currentTarget as HTMLSelectElement).value)
                        }
                      >
                        <option value="">Pick a commit…</option>
                        @for (const c of commits; key `s:${c.sha}`) {
                          <option value={c.sha}>
                            {`${c.short_sha} — ${c.subject}` as string}
                          </option>;
                        }
                      </select>
                    </label>
                    <label className="block text-[12px] font-medium text-foreground">
                      End commit
                      <select
                        className="mt-1 w-full rounded-md border border-border bg-background px-1.5 py-1 text-[12px]"
                        value={rangeEnd}
                        suppressNativeChangeWarning
                        data-testid="pull-v2-range-end"
                        onChange={(e) =>
                          pickRangeEnd((e.currentTarget as HTMLSelectElement).value)
                        }
                      >
                        <option value="">Pick a commit…</option>
                        @for (const c of commits; key `e:${c.sha}`) {
                          <option value={c.sha}>
                            {`${c.short_sha} — ${c.subject}` as string}
                          </option>;
                        }
                      </select>
                    </label>
                  </div>
                </>;
              } @else {
                <>
                  <DropdownMenuLinkItem
                    href={`${base}/changes` as string}
                    data-testid="pull-v2-scope-all"
                    onClick={() => setSinceActive(false)}
                  >
                    <Check
                      aria-hidden
                      size={13}
                      className={scope.kind === "none" ? "" : "invisible"}
                    />
                    All commits
                  </DropdownMenuLinkItem>
                  @if (props.canInteract) {
                    <DropdownMenuItem
                      data-testid="pull-v2-scope-since"
                      onClick={() => setSinceActive(true)}
                    >
                      <Check
                        aria-hidden
                        size={13}
                        className={scope.kind === "since" ? "" : "invisible"}
                      />
                      Changes since your last review
                    </DropdownMenuItem>;
                  }
                  <DropdownMenuItem
                    closeOnClick={false}
                    data-testid="pull-v2-scope-range"
                    onClick={() => setRangeMode(true)}
                  >
                    <Check aria-hidden size={13} className="invisible" />
                    Select a range
                  </DropdownMenuItem>
                  @if (commits.length > 0) {
                    <DropdownMenuSeparator />;
                  }
                  @for (const c of commits; key c.sha) {
                    <DropdownMenuLinkItem
                      href={`${base}/changes/${c.sha}` as string}
                      data-testid="pull-v2-scope-commit"
                      onClick={() => setSinceActive(false)}
                    >
                      <Check
                        aria-hidden
                        size={13}
                        className={
                          scope.kind === "sha" && scope.sha === c.sha ? "" : "invisible"
                        }
                      />
                      <span className="min-w-0 truncate">
                        <span className="font-mono">{c.short_sha as string}</span>{" "}
                        {c.subject as string}
                      </span>
                    </DropdownMenuLinkItem>;
                  }
                </>;
              }
            </DropdownMenuContent>
          </DropdownMenu>
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
          @if (marksEnabled) {
            <span
              className="ml-auto text-[12px] text-muted-foreground"
              data-testid="pull-v2-viewed-progress"
            >
              {`${viewedInFiles} of ${files.length} files viewed` as string}
            </span>;
          }
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
                panelOpen()
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              ) as string
            }
            aria-label="Toggle comments panel"
            aria-expanded={panelOpen()}
            data-testid="pull-v2-comments-toggle"
            onClick={() => setPanelOpen(!panelOpen())}
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
              <DropdownMenuCheckboxItem
                checked={minimizeComments}
                onCheckedChange={(v) => setMinimizeComments(v)}
                data-testid="pull-v2-minimize-comments"
              >
                Minimize comments
              </DropdownMenuCheckboxItem>
              <DropdownMenuSeparator />
              <DropdownMenuCheckboxItem
                checked={hideWhitespace}
                onCheckedChange={(v) => setHideWhitespace(v)}
                data-testid="pull-v2-hide-whitespace"
              >
                Hide whitespace changes
              </DropdownMenuCheckboxItem>
              <DropdownMenuSeparator />
              @for (const n of [0, 3, 10, 20]; key n) {
                <DropdownMenuItem
                  data-testid={`pull-v2-context-${n}`}
                  onClick={() => setContextLines(n === 3 ? null : n)}
                >
                  <Check
                    aria-hidden
                    size={13}
                    className={(contextLines ?? 3) === n ? "" : "invisible"}
                  />
                  {`Context: ${n} lines` as string}
                </DropdownMenuItem>;
              }
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
                      owner={props.owner}
                      repoName={props.repoName}
                      file={f}
                      view={view}
                      wrap={wrap}
                      compact={compact}
                      expanded={!(collapsed[f.path] ?? false)}
                      viewed={viewedSet.has(f.path)}
                      marksEnabled={marksEnabled}
                      canInteract={props.canInteract}
                      canWrite={props.canWrite ?? false}
                      minimizeComments={minimizeComments}
                      pending={pendingComments.filter((c) => c.path === f.path)}
                      submitted={submittedComments.filter((c) => c.path === f.path)}
                      draft={draft?.path === f.path ? draft : null}
                      draftBody={draftBody}
                      draftBusy={draftBusy}
                      draftError={draftError}
                      viewFileHref={blobHref(
                        props.owner,
                        props.repoName,
                        viewFileRef,
                        f.path,
                      )}
                      onToggleExpand={() => toggleDirFile(f.path)}
                      onToggleViewed={(v) => void toggleViewed(f.path, v)}
                      onDraftBody={setDraftBody}
                      onStartDraft={startDraft}
                      onSubmitDraft={(p) => void submitDraft(p)}
                      onCancelDraft={() => {
                        setDraft(null);
                        setDraftBody("");
                        setDraftError(null);
                      }}
                      onDiscardPending={(id) => void discardPending(id)}
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
    </div>

    @if (props.canWrite) {
      <div className="sticky bottom-0 z-10">
        <PullV2ReviewBar
          pending={pendingComments}
          isAuthor={props.isAuthor}
          busy={reviewBusy}
          error={reviewError}
          onSubmit={(state, body) => void submitReview(state, body)}
          onDiscardPending={(id) => void discardPending(id)}
        />
      </div>;
    }
  </div>;
}
