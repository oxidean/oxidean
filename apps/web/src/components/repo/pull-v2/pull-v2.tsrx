import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  CheckSuitePublic,
  PullCommentPublic,
  PullDiffFile,
  PullPublic,
  RepoPublic,
} from "@oxidean/api-client";
import {
  Check,
  ChevronDown,
  Copy,
  FileText,
  GitBranch,
  GitMerge,
  GitPullRequest,
  GitPullRequestClosed,
  GitPullRequestDraft,
  MessageSquare,
  PanelRight,
  PanelRightClose,
  Pencil,
  X,
} from "@octanejs/lucide";
import { useEffect, useMemo, useState } from "octane";
import { PullV2ActivityPane } from "@/components/repo/pull-v2/pull-v2-activity-pane";
import { PullV2Changes } from "@/components/repo/pull-v2/pull-v2-changes";
import { PullV2Checks } from "@/components/repo/pull-v2/pull-v2-checks";
import { PullV2CommentsPanel } from "@/components/repo/pull-v2/pull-v2-comments-panel";
import { PullV2Commits } from "@/components/repo/pull-v2/pull-v2-commits";
import { PullV2MergeCard } from "@/components/repo/pull-v2/pull-v2-merge-card";
import { PullV2Overview } from "@/components/repo/pull-v2/pull-v2-overview";
import { PullV2Rail } from "@/components/repo/pull-v2/pull-v2-rail";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { apiClient } from "@/lib/api-client";
import { useSearchString } from "@/lib/pathname.tsrx";
import { formatRelative } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import { fileAnchorId } from "./pull-v2-diff-utils";

type PullV2Tab = "overview" | "commits" | "checks" | "changes";

/** Sub-path after `/pull/N` → tab. `changes/<sha>` is the files tab too;
 * `files` is the client-side alias for the server `/files` → `/changes` 302. */
export function pullV2TabFromSplat(splat: string): PullV2Tab {
  if (splat === "commits") return "commits";
  if (splat === "checks") return "checks";
  if (
    splat === "changes" ||
    splat.startsWith("changes/") ||
    splat === "files" ||
    splat.startsWith("files/")
  )
    return "changes";
  return "overview";
}

type PullDetailV2Props = {
  owner: string;
  repoName: string;
  number: number;
  repo: RepoPublic;
  pull: PullPublic;
  canWrite: boolean;
  canInteract: boolean;
  isAuthor: boolean;
  currentUserId: string;
  tab: PullV2Tab;
  /** `changes/<sha>` scoped commit when the files tab carries one. */
  changesSha?: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** Right-pane views — GitHub's new UI "Description ▾" pane selector. Panes
 * with no backend domain (Alerts, Copilot, batch review) stay unlisted. */
type PullPane = "description" | "activity" | "comments" | "merge";

const PANES: { id: PullPane; label: string }[] = [
  { id: "activity", label: "Activity" },
  { id: "comments", label: "Comments" },
  { id: "description", label: "Description" },
  { id: "merge", label: "Merge status" },
];

function paneLabel(pane: PullPane): string {
  return PANES.find((p) => p.id === pane)?.label ?? "Description";
}

function paneFromParam(value: string | null): PullPane | null {
  if (value === "comments") return "comments";
  if (value === "activity") return "activity";
  if (value === "merge") return "merge";
  if (value === "description" || value === "overview") return "description";
  return null;
}

function pullStateBadge(state: string, draft: boolean) @{
  @if (state === "merged") {
    <Badge variant="merged">
      <GitMerge aria-hidden size={12} />
      Merged
    </Badge>;
  } @else if (state === "closed") {
    <Badge variant="closed">
      <GitPullRequestClosed aria-hidden size={12} />
      Closed
    </Badge>;
  } @else {
    @if (draft) {
      <Badge variant="draft">
        <GitPullRequestDraft aria-hidden size={12} />
        Draft
      </Badge>;
    } @else {
      <Badge variant="open">
        <GitPullRequest aria-hidden size={12} />
        Open
      </Badge>;
    }
  }
}

/**
 * GitHub's `Open ▾` state pill — the caret opens the state-change menu
 * (draft toggle / close / reopen) when the viewer can write. Merged pulls
 * and read-only viewers get a static pill.
 */
function StatePillMenu(props: {
  state: string;
  draft: boolean;
  canWrite: boolean;
  busy: boolean;
  onConvert: (draft: boolean) => void;
  onClose: () => void;
  onReopen: () => void;
}) @{
  const actionable = props.canWrite && (props.state === "open" || props.state === "closed");

  @if (!actionable) {
    <span data-testid="pull-v2-state-pill">{pullStateBadge(props.state, props.draft)}</span>;
  } @else {
    <DropdownMenu>
      <DropdownMenuTrigger
        className="inline-flex cursor-pointer items-center gap-0.5 rounded-full focus-visible:outline-2 focus-visible:outline-primary"
        aria-label="Pull request state actions"
        aria-haspopup="menu"
        data-testid="pull-v2-state-menu"
        disabled={props.busy}
      >
        {pullStateBadge(props.state, props.draft)}
        <ChevronDown aria-hidden size={12} className="-ml-0.5 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-48">
        @if (props.state === "open") {
          <>
            @if (props.draft) {
              <DropdownMenuItem
                data-testid="pull-v2-state-ready"
                onClick={() => props.onConvert(false)}
              >
                <GitPullRequest aria-hidden size={13} />
                Ready for review
              </DropdownMenuItem>;
            } @else {
              <DropdownMenuItem
                data-testid="pull-v2-state-draft"
                onClick={() => props.onConvert(true)}
              >
                <GitPullRequestDraft aria-hidden size={13} />
                Convert to draft
              </DropdownMenuItem>;
            }
            <DropdownMenuItem
              data-testid="pull-v2-state-close"
              onClick={() => props.onClose()}
            >
              <GitPullRequestClosed aria-hidden size={13} />
              Close pull request
            </DropdownMenuItem>
          </>;
        } @else {
          <DropdownMenuItem
            data-testid="pull-v2-state-reopen"
            onClick={() => props.onReopen()}
          >
            <GitPullRequest aria-hidden size={13} />
            Reopen pull request
          </DropdownMenuItem>;
        }
      </DropdownMenuContent>
    </DropdownMenu>;
  }
}

/** GitHub's five-square diffstat visual — green share = additions, red =
 * deletions, muted = untouched remainder. */
function DiffStatDots(props: { adds: number; dels: number }) @{
  const total = props.adds + props.dels;
  let green = 0;
  let red = 0;
  if (total > 0) {
    green = Math.round((props.adds / total) * 5);
    red = Math.round((props.dels / total) * 5);
    if (green + red > 5) {
      if (green >= red) green = 5 - red;
      else red = 5 - green;
    }
    if (props.adds > 0 && green === 0) green = 1;
    if (props.dels > 0 && red === 0) red = 1;
    if (green + red > 5) red = 5 - green;
  }
  const squares: ("add" | "del" | "empty")[] = [];
  for (let i = 0; i < 5; i += 1) {
    if (i < green) squares.push("add");
    else if (i < green + red) squares.push("del");
    else squares.push("empty");
  }

  <span className="inline-flex items-center gap-0.5" aria-hidden data-testid="pull-v2-diffdots">
    @for (const [i, s] of squares.entries(); key i) {
      <span
        className={
          cn(
            "inline-block size-2.5 rounded-[2px]",
            s === "add" ? "bg-success" : s === "del" ? "bg-destructive" : "bg-muted",
          ) as string
        }
      />;
    }
  </span>;
}

const TABS: { id: PullV2Tab; label: string; path: (base: string) => string }[] = [
  { id: "overview", label: "Overview", path: (b) => b },
  { id: "commits", label: "Commits", path: (b) => `${b}/commits` },
  { id: "checks", label: "Checks", path: (b) => `${b}/checks` },
  { id: "changes", label: "Files changed", path: (b) => `${b}/changes` },
];

/** Sum +/− content lines across pull.files patches (skip @@ hunks and +++/--- markers). */
function diffStatTotals(files: PullDiffFile[]): { adds: number; dels: number } {
  let adds = 0;
  let dels = 0;
  for (const f of files) {
    for (const line of (f.patch ?? "").split("\n")) {
      if (line.startsWith("+++") || line.startsWith("---") || line.startsWith("@@")) continue;
      if (line.startsWith("+")) adds += 1;
      else if (line.startsWith("-")) dels += 1;
    }
  }
  return { adds, dels };
}

function scrollToFile(path: string) {
  const el = document.getElementById(fileAnchorId(path));
  if (el && typeof el.scrollIntoView === "function") {
    el.scrollIntoView({ block: "start", behavior: "smooth" });
  }
}

/** GitHub-style branch pill + copy-button pair for the base/head merge row. */
function BranchChip(props: { label: string; name: string; testid: string }) @{
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(props.name);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable — manual copy */
    }
  }

  <span className="inline-flex items-center gap-0.5">
    <span
      className="inline-flex items-center gap-1 rounded-md border border-border bg-muted/50 px-1.5 py-0.5 font-mono text-[12px] text-foreground"
      data-testid={props.testid as string}
    >
      <GitBranch aria-hidden size={11} className="text-muted-foreground" />
      {props.name as string}
    </span>
    <button
      type="button"
      className="inline-flex size-6 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
      aria-label={`Copy ${props.label} branch name` as string}
      data-testid={`${props.testid}-copy` as string}
      onClick={() => void copy()}
    >
      @if (copied) {
        <Check aria-hidden size={12} className="text-success" />;
      } @else {
        <Copy aria-hidden size={12} />;
      }
    </button>
  </span>;
}

type MergeChip = { variant: "destructive" | "primary" | "success" | "amber"; label: string };

/**
 * pr-new-ui (WI-0001): GitHub-style pull chrome — sticky mini-header with
 * state chip, copy-link / submit-review / comments-panel actions, URL-backed
 * tab nav with count badges, and the base/head branch row. Tab bodies
 * delegate to the pull-v2 surfaces (stages 4–10); the legacy v1 components
 * stay untouched for flag-off users.
 */
export function PullDetailV2(props: PullDetailV2Props) @{
  const queryClient = useQueryClient();
  const searchStr = useSearchString();
  const base = `/${props.owner}/${props.repoName}/pull/${props.number}`;
  const pullState = (props.pull.state as string) ?? "open";
  // can_edit_pull permits the author too — title/base/body edits and
  // close/reopen/draft affordances follow canEdit; write-gated RPCs
  // (reviewers, labels, assignees, merge, review submit) stay on canWrite.
  const canEdit = props.canWrite || props.isAuthor;
  const headRef =
    props.pull.head_owner === props.owner && props.pull.head_name === props.repoName
      ? props.pull.head_ref
      : `${props.pull.head_owner}:${props.pull.head_ref}`;

  const [copied, setCopied] = useState(false);
  const [paneOpen, setPaneOpen] = useState(true);
  const [pane, setPane] = useState<PullPane>("description");
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [titleBusy, setTitleBusy] = useState(false);
  const [titleError, setTitleError] = useState("");
  const [stateBusy, setStateBusy] = useState(false);
  const [stateError, setStateError] = useState("");

  // Badge/panel queries reuse the tab bodies' keys + queryFns — shared cache,
  // no second fetch when a tab mounts the same key.
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

  const filesQ = useQuery({
    queryKey: [
      "pull",
      "files",
      props.owner,
      props.repoName,
      props.number,
      "all",
      false,
      null,
    ] as const,
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

  const suitesQ = useQuery({
    queryKey: [
      "check",
      "suites",
      "list",
      props.owner,
      props.repoName,
      props.pull.head_sha,
    ] as const,
    queryFn: async () => {
      const res = await apiClient.check.suites.list({
        owner: props.owner,
        name: props.repoName,
        head_sha: props.pull.head_sha,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!props.pull.head_sha,
  });

  const runsQ = useQuery({
    queryKey: [
      "check",
      "runs",
      "listForRef",
      props.owner,
      props.repoName,
      props.pull.head_sha,
    ] as const,
    queryFn: async () => {
      const res = await apiClient.check.runs.listForRef({
        owner: props.owner,
        name: props.repoName,
        ref: props.pull.head_sha,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!props.pull.head_sha && pullState === "open",
  });

  const branchQ = useQuery({
    queryKey: ["pull", "branchStatus", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.branchStatus({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: pullState === "open",
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

  // `?panel=<pane>` deep-links a right pane (comments / activity / merge /
  // description) on load and soft-nav.
  useEffect(() => {
    const p = paneFromParam(new URLSearchParams(searchStr).get("panel"));
    if (p) {
      setPane(p);
      setPaneOpen(true);
    }
  }, [searchStr]);

  const commentsOpen = pane === "comments" && paneOpen;

  function setCommentsOpen(open: boolean) {
    if (open) {
      setPane("comments");
      setPaneOpen(true);
      syncPaneParam("comments", true);
    } else {
      setPaneOpen(false);
      syncPaneParam("comments", false);
    }
  }

  function selectPane(next: PullPane) {
    setPane(next);
    setPaneOpen(true);
    syncPaneParam(next, true);
  }

  function closePane() {
    setPaneOpen(false);
    syncPaneParam(pane, false);
  }

  /** `?panel=` mirrors the open non-default pane; the default description
   * pane (and a closed pane) carry no param, matching the pre-pane
   * `?panel=comments` deep-link contract. */
  function syncPaneParam(nextPane = pane, open = paneOpen) {
    if (typeof window === "undefined") return;
    const url = new URL(window.location.href);
    if (open && nextPane !== "description") url.searchParams.set("panel", nextPane);
    else url.searchParams.delete("panel");
    window.history.replaceState(null, "", `${url.pathname}${url.search}${url.hash}`);
  }

  const comments: PullCommentPublic[] = commentsQ.data?.comments ?? [];

  const suites: CheckSuitePublic[] = suitesQ.data?.suites ?? [];
  const checkCount = suitesQ.data ? suites.reduce((n, s) => n + s.check_runs_count, 0) : undefined;
  const commitCount = commitsQ.data?.commits.length;
  const fileCount = filesQ.data?.files.length;
  const tabCounts: Partial<Record<PullV2Tab, number>> = {
    commits: commitCount,
    checks: checkCount,
    changes: fileCount,
  };

  const checkRuns = runsQ.data?.check_runs ?? [];
  const failingChecks = checkRuns.filter(
    (r) =>
      r.status === "completed" &&
      !!r.conclusion &&
      !["success", "neutral", "skipped"].includes(r.conclusion),
  ).length;
  const pendingChecks = checkRuns.filter((r) => r.status !== "completed").length;
  const behindBy = branchQ.data?.status === "behind" ? (branchQ.data.behind_count ?? 0) : 0;

  const mergeChip: MergeChip | null =
    pullState !== "open"
      ? null
      : failingChecks > 0
        ? { variant: "destructive", label: "Checks failing" }
        : pendingChecks > 0
          ? { variant: "primary", label: "Checks pending" }
          : behindBy > 0
            ? { variant: "amber", label: "Behind base" }
            : checkRuns.length > 0
              ? { variant: "success", label: "Checks passing" }
              : null;

  const stats = useMemo(() => diffStatTotals(filesQ.data?.files ?? []), [filesQ.data]);

  async function copyLink() {
    try {
      await navigator.clipboard.writeText(window.location.href);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  }

  async function invalidatePull() {
    await queryClient.invalidateQueries({
      queryKey: ["pull", "get", props.owner, props.repoName, props.number],
    });
  }

  async function stateAction(fn: () => Promise<{ ok: boolean; error?: { message: string } }>) {
    setStateBusy(true);
    setStateError("");
    try {
      const res = await fn();
      setStateBusy(false);
      if (!res.ok) {
        setStateError(res.error?.message || "Could not update the pull request.");
        return;
      }
      await invalidatePull();
    } catch {
      setStateBusy(false);
      setStateError(NETWORK_ERROR);
    }
  }

  const convertDraft = (draft: boolean) =>
    void stateAction(() =>
      apiClient.pull.update({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        draft,
      }),
    );
  const closePull = () =>
    void stateAction(() =>
      apiClient.pull.close({ owner: props.owner, name: props.repoName, number: props.number }),
    );
  const reopenPull = () =>
    void stateAction(() =>
      apiClient.pull.reopen({ owner: props.owner, name: props.repoName, number: props.number }),
    );
  const retargetBase = (base_ref: string) =>
    void stateAction(async () => {
      const res = await apiClient.pull.update({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        base_ref,
      });
      if (res.ok) {
        // Base change rewrites the diff + marks line comments outdated —
        // refresh everything pull-scoped, not just pull.get.
        const keys = ["commits", "files", "comments", "reviews", "branchStatus"] as const;
        for (const k of keys) {
          void queryClient.invalidateQueries({
            queryKey: ["pull", k, props.owner, props.repoName, props.number],
          });
        }
      }
      return res;
    });

  const refsQ = useQuery({
    queryKey: ["repo", "refs", props.owner, props.repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.refs({ owner: props.owner, name: props.repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: props.canWrite && pullState === "open",
  });
  const baseChoices = (refsQ.data?.refs ?? [])
    .map((r) => r.name.replace(/^refs\/heads\//, ""))
    .filter((n) => n && n !== props.pull.base_ref);

  function startTitleEdit() {
    setTitleDraft(props.pull.title);
    setTitleError("");
    setEditingTitle(true);
  }

  async function saveTitle() {
    const title = titleDraft.trim();
    if (!title || title === props.pull.title) {
      setEditingTitle(false);
      return;
    }
    setTitleBusy(true);
    setTitleError("");
    try {
      const res = await apiClient.pull.update({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
        title,
      });
      setTitleBusy(false);
      if (!res.ok) {
        setTitleError(res.error.message || "Could not update the title");
        return;
      }
      setEditingTitle(false);
      await queryClient.invalidateQueries({
        queryKey: ["pull", "get", props.owner, props.repoName, props.number],
      });
    } catch {
      setTitleBusy(false);
      setTitleError(NETWORK_ERROR);
    }
  }

  <div className="space-y-4">
    {/* Row 1 — mini-header: #N, truncated title, copy, status + pane actions.
        Sticks to the top like GitHub's pull mini-header. */}
    <div className="sticky top-0 z-20 -mx-4 border-b border-border bg-background/95 px-4 backdrop-blur sm:-mx-6 sm:px-6">
      <div className="flex flex-wrap items-center gap-2 py-2 text-[14px]">
        <span className="font-mono text-muted-foreground">#{props.number as number}</span>
        <h2 className="min-w-0 max-w-[55%] truncate font-[family-name:var(--font-display)] font-semibold">
          {props.pull.title as string}
        </h2>
        <button
          type="button"
          className="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label="Copy link"
          data-testid="pull-v2-copy-link"
          onClick={() => void copyLink()}
        >
          @if (copied) {
            <Check aria-hidden size={13} className="text-success" />;
          } @else {
            <Copy aria-hidden size={13} />;
          }
        </button>
        <div className="ml-auto flex items-center gap-1">
          @if (mergeChip) {
            <AppLink
              href={`${base}?panel=merge` as string}
              className="no-underline"
              title="Merge status — view the merge card on Overview"
              data-testid="pull-v2-merge-state"
            >
              <Badge
                variant={mergeChip.variant === "amber" ? "outline" : mergeChip.variant}
                className={
                  cn(
                    "cursor-pointer",
                    mergeChip.variant === "amber"
                      ? "border-amber-500/50 bg-amber-500/10 text-amber-600 dark:text-amber-400"
                      : "",
                  ) as string
                }
              >
                {mergeChip.label as string}
              </Badge>
            </AppLink>;
          }
          @if (props.canInteract && pullState === "open") {
            <AppLink
              href={`${base}/changes` as string}
              className={cn(buttonVariants({ size: "sm" }), "h-7 px-2 text-[12px]") as string}
              data-testid="pull-v2-submit-review"
            >
              Submit review
            </AppLink>;
          }
          <button
            type="button"
            className={
              cn(
                "inline-flex size-7 items-center justify-center rounded-sm",
                commentsOpen
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              ) as string
            }
            aria-label="Toggle comments panel"
            aria-expanded={commentsOpen}
            data-testid="pull-v2-panel-toggle"
            onClick={() => setCommentsOpen(!commentsOpen)}
          >
            <MessageSquare aria-hidden size={15} />
          </button>
          <button
            type="button"
            className={
              cn(
                "inline-flex size-7 items-center justify-center rounded-sm",
                paneOpen
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              ) as string
            }
            aria-label="Toggle sidebar"
            aria-expanded={paneOpen}
            data-testid="pull-v2-rail-toggle"
            onClick={() => {
              const next = !paneOpen;
              setPaneOpen(next);
              syncPaneParam(pane, next);
            }}
          >
            @if (paneOpen) {
              <PanelRightClose aria-hidden size={15} />;
            } @else {
              <PanelRight aria-hidden size={15} />;
            }
          </button>
        </div>
      </div>
      @if (titleError) {
        <p
          className="pb-1 text-[12px] text-destructive"
          role="alert"
          data-testid="pull-v2-title-error"
        >
          {titleError as string}
        </p>;
      }
    </div>

    {/* Row 2 — pane toolbar: GitHub's "Description ▾" selector + edit + close
        sits above the right pane. Preview/code/notifications controls have no
        backend domain here and stay out rather than rendering dead buttons. */}
    <div
      className="hidden items-center justify-end gap-1 lg:flex"
      data-testid="pull-v2-pane-toolbar"
    >
      <DropdownMenu>
        <DropdownMenuTrigger
          className="inline-flex h-7 cursor-pointer items-center gap-1 rounded-md border border-border bg-background px-2 text-[13px] text-foreground hover:bg-muted"
          aria-label="Select right pane"
          aria-haspopup="menu"
          data-testid="pull-v2-pane-select"
        >
          <FileText aria-hidden size={13} className="text-muted-foreground" />
          {paneLabel(pane) as string}
          <ChevronDown aria-hidden size={12} className="text-muted-foreground" />
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-44">
          @for (const p of PANES; key p.id) {
            <DropdownMenuItem
              data-testid={`pull-v2-pane-${p.id}` as string}
              onClick={() => selectPane(p.id)}
            >
              <Check aria-hidden size={13} className={pane === p.id ? "" : "invisible"} />
              {p.label as string}
            </DropdownMenuItem>;
          }
        </DropdownMenuContent>
      </DropdownMenu>
      @if (canEdit && pullState === "open") {
        <button
          type="button"
          className="inline-flex size-7 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label="Edit title"
          data-testid="pull-v2-title-edit"
          onClick={startTitleEdit}
        >
          <Pencil aria-hidden size={14} />
        </button>;
      }
      @if (paneOpen) {
        <button
          type="button"
          className="inline-flex size-7 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label="Close right pane"
          data-testid="pull-v2-pane-close"
          onClick={closePane}
        >
          <X aria-hidden size={14} />
        </button>;
      }
    </div>

    {/* Row 3 — title block: GitHub's large H1 with the inline-edit pencil. */}
    <div data-testid="pull-v2-title-block">
      @if (editingTitle) {
        <form
          method="post"
          action="#"
          className="flex min-w-0 items-center gap-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            void saveTitle();
          }}
        >
          <Input
            value={titleDraft}
            className="h-9 min-w-0 flex-1 text-[20px] font-semibold"
            disabled={titleBusy}
            aria-label="Pull request title"
            data-testid="pull-v2-title-input"
            onInput={(e) => setTitleDraft((e.currentTarget as HTMLInputElement).value)}
          />
          <Button
            type="submit"
            size="sm"
            disabled={titleBusy || !titleDraft.trim()}
            data-testid="pull-v2-title-save"
          >
            Save
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={titleBusy}
            data-testid="pull-v2-title-cancel"
            onClick={() => {
              setEditingTitle(false);
              setTitleError("");
            }}
          >
            Cancel
          </Button>
        </form>;
      } @else {
        <div className="flex items-start gap-2">
          <h1 className="min-w-0 font-[family-name:var(--font-display)] text-[24px] font-semibold leading-tight text-foreground">
            {props.pull.title as string}
          </h1>
          @if (props.canWrite && pullState === "open") {
            <button
              type="button"
              className="mt-1.5 inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
              aria-label="Edit title"
              onClick={startTitleEdit}
            >
              <Pencil aria-hidden size={14} />
            </button>;
          }
        </div>;
      }
    </div>

    {/* Row 4 — state row: state pill (with actions), author + opened-at,
        base ← head ref chips, and the +/− diffstat with GitHub's five dots. */}
    <div
      className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-muted-foreground"
      data-testid="pull-v2-state-row"
    >
      <StatePillMenu
        state={pullState}
        draft={props.pull.draft}
        canWrite={canEdit}
        busy={stateBusy}
        onConvert={convertDraft}
        onClose={closePull}
        onReopen={reopenPull}
      />
      <span
        className="inline-flex size-5 items-center justify-center rounded-full bg-muted text-[10px] font-semibold text-muted-foreground"
        aria-hidden
      >
        {(props.pull.author_username || "?").slice(0, 1).toUpperCase() as string}
      </span>
      <span>
        <span className="font-medium text-foreground">{props.pull.author_username as string}</span>
        {
          (pullState === "merged" && props.pull.merged_at
            ? ` merged ${formatRelative(props.pull.merged_at)}`
            : ` opened ${formatRelative(props.pull.created_at)}`) as string
        }
      </span>
      <span className="inline-flex flex-wrap items-center gap-1.5">
        @if (
          canEdit &&
          pullState === "open" &&
          baseChoices.length > 0
        ) {
          <DropdownMenu>
            <DropdownMenuTrigger
              className="inline-flex cursor-pointer items-center gap-0.5 rounded-md border border-border bg-muted/50 px-1.5 py-0.5 font-mono text-[12px] text-foreground hover:bg-muted"
              aria-label="Change base branch"
              aria-haspopup="menu"
              data-testid="pull-v2-base-menu"
              disabled={stateBusy}
            >
              <GitBranch aria-hidden size={11} className="text-muted-foreground" />
              {props.pull.base_ref as string}
              <ChevronDown aria-hidden size={11} className="text-muted-foreground" />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="min-w-44">
              @for (const b of baseChoices; key b) {
                <DropdownMenuItem
                  data-testid={`pull-v2-base-${b}` as string}
                  onClick={() => retargetBase(b)}
                >
                  {b as string}
                </DropdownMenuItem>;
              }
            </DropdownMenuContent>
          </DropdownMenu>;
        } @else {
          <span
            className="inline-flex items-center gap-1 rounded-md border border-border bg-muted/50 px-1.5 py-0.5 font-mono text-[12px] text-foreground"
            data-testid="pull-v2-base-branch"
          >
            <GitBranch aria-hidden size={11} className="text-muted-foreground" />
            {props.pull.base_ref as string}
          </span>;
        }
        <span aria-hidden>←</span>
        <BranchChip label="head" name={headRef} testid="pull-v2-head-branch" />
      </span>
      @if (filesQ.data) {
        <span
          className="ml-auto inline-flex items-center gap-1.5 font-mono text-[13px]"
          data-testid="pull-v2-diffstat"
        >
          <span className="text-success">{`+${stats.adds.toLocaleString()}` as string}</span>{" "}
          <span className="text-destructive">
            {`−${stats.dels.toLocaleString()}` as string}
          </span>
          <DiffStatDots adds={stats.adds} dels={stats.dels} />
        </span>;
      }
    </div>
    @if (stateError) {
      <p
        className="text-[12px] text-destructive"
        role="alert"
        data-testid="pull-v2-state-error"
      >
        {stateError as string}
      </p>;
    }

    {/* Row 5 — tab nav (sticky under the mini-header on scroll). */}
    <nav
      className="sticky top-10 z-10 -mx-4 flex gap-1 overflow-x-auto border-b border-border bg-background/95 px-4 backdrop-blur sm:-mx-6 sm:px-6"
      aria-label="Pull request sections"
    >
      @for (const t of TABS; key t.id) {
        <AppLink
          href={t.path(base) as string}
          className={
            cn(
              "border-b-2 px-3 pb-2 pt-1 text-sm no-underline",
              props.tab === t.id
                ? "border-primary font-semibold text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground",
            ) as string
          }
          aria-current={props.tab === t.id ? "page" : undefined}
          data-testid={`pull-v2-tab-${t.id}`}
        >
          {t.label as string}
          @if (tabCounts[t.id] !== undefined) {
            <span
              className="ml-1 inline-flex items-center rounded-full bg-muted px-1.5 py-0.5 text-[11px] font-normal leading-none text-muted-foreground"
              data-testid={`pull-v2-tab-count-${t.id}` as string}
            >
              {tabCounts[t.id] as number}
            </span>;
          }
        </AppLink>;
      }
    </nav>

    {/* Row 6 — tab body + unified right pane (all tabs, like GitHub). */}
    <div className="flex items-start gap-6">
      <div className="min-w-0 flex-1 space-y-4">
        @if (props.tab === "overview") {
          <PullV2Overview
            owner={props.owner}
            repoName={props.repoName}
            number={props.number}
            repo={props.repo}
            pull={props.pull}
            canWrite={props.canWrite}
            canInteract={props.canInteract}
            isAuthor={props.isAuthor}
            currentUserId={props.currentUserId}
          />;
        }
        @if (props.tab === "commits") {
          <PullV2Commits
            owner={props.owner}
            repoName={props.repoName}
            number={props.number}
          />;
        }
        @if (props.tab === "checks") {
          <PullV2Checks
            owner={props.owner}
            repoName={props.repoName}
            sha={props.pull.head_sha}
          />;
        }
        @if (props.tab === "changes") {
          <PullV2Changes
            owner={props.owner}
            repoName={props.repoName}
            number={props.number}
            canInteract={props.canInteract}
            canWrite={props.canWrite}
            isAuthor={props.isAuthor}
            currentUserId={props.currentUserId}
            changesSha={props.changesSha}
            headSha={props.pull.head_sha}
            panelOpen={commentsOpen}
            onPanelToggle={setCommentsOpen}
          />;
        }
      </div>

      @if (paneOpen) {
        <aside
          className="sticky top-24 hidden max-h-[calc(100vh-8rem)] w-80 shrink-0 self-start overflow-y-auto lg:block"
          aria-label={`${paneLabel(pane)} pane`}
          data-testid="pull-v2-pane"
        >
          @if (pane === "description") {
            <PullV2Rail
              owner={props.owner}
              repoName={props.repoName}
              number={props.number}
              pull={props.pull}
              canWrite={props.canWrite}
              canEdit={canEdit}
            />;
          }
          @if (pane === "comments") {
            <div className="overflow-hidden rounded-md border border-border bg-card">
              <PullV2CommentsPanel
                comments={comments}
                currentUserId={props.currentUserId}
                owner={props.owner}
                repoName={props.repoName}
                number={props.number}
                canInteract={props.canInteract}
                onClose={() => setCommentsOpen(false)}
                onJump={scrollToFile}
              />
            </div>;
          }
          @if (pane === "activity") {
            <PullV2ActivityPane
              owner={props.owner}
              repoName={props.repoName}
              number={props.number}
              onJump={scrollToFile}
            />;
          }
          @if (pane === "merge") {
            <PullV2MergeCard
              owner={props.owner}
              repoName={props.repoName}
              number={props.number}
              canWrite={props.canWrite}
              pullState={pullState}
              draft={props.pull.draft}
              headSha={props.pull.head_sha}
              mergedAt={props.pull.merged_at ?? null}
              mergedBy={props.pull.merged_by ?? null}
              closedAt={props.pull.closed_at ?? null}
              headOwner={props.pull.head_owner}
            />;
          }
        </aside>;
      }
    </div>
  </div>;
}
