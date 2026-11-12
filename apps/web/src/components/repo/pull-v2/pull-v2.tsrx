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
  Copy,
  GitBranch,
  GitMerge,
  GitPullRequest,
  GitPullRequestClosed,
  GitPullRequestDraft,
  MessageSquare,
  Pencil,
} from "@octanejs/lucide";
import { useEffect, useMemo, useState } from "octane";
import { PullV2Changes } from "@/components/repo/pull-v2/pull-v2-changes";
import { PullV2Checks } from "@/components/repo/pull-v2/pull-v2-checks";
import { PullV2CommentsPanel } from "@/components/repo/pull-v2/pull-v2-comments-panel";
import { PullV2Commits } from "@/components/repo/pull-v2/pull-v2-commits";
import { PullV2Overview } from "@/components/repo/pull-v2/pull-v2-overview";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { apiClient } from "@/lib/api-client";
import { useSearchString } from "@/lib/pathname.tsrx";
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
  const headRef =
    props.pull.head_owner === props.owner && props.pull.head_name === props.repoName
      ? props.pull.head_ref
      : `${props.pull.head_owner}:${props.pull.head_ref}`;

  const [copied, setCopied] = useState(false);
  const [panelOpen, setPanelOpen] = useState(false);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [titleBusy, setTitleBusy] = useState(false);
  const [titleError, setTitleError] = useState("");

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

  // `?panel=comments` opens the comments panel on load / soft-nav.
  useEffect(() => {
    if (new URLSearchParams(searchStr).get("panel") === "comments") setPanelOpen(true);
  }, [searchStr]);

  const comments: PullCommentPublic[] = commentsQ.data?.comments ?? [];
  const pendingReviewCount = comments.filter((c) => c.pending).length;

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

  function setPanel(next: boolean) {
    setPanelOpen(next);
    if (typeof window === "undefined") return;
    const url = new URL(window.location.href);
    if (next) url.searchParams.set("panel", "comments");
    else url.searchParams.delete("panel");
    window.history.replaceState(null, "", `${url.pathname}${url.search}${url.hash}`);
  }

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
    <div className="sticky top-0 z-20 -mx-4 border-b border-border bg-background/95 px-4 backdrop-blur sm:-mx-6 sm:px-6">
      <div className="flex flex-wrap items-center gap-2 py-2 text-[14px]">
        <span className="font-mono text-muted-foreground">#{props.number as number}</span>
        @if (editingTitle) {
          <form
            method="post"
            action="#"
            className="flex min-w-0 flex-1 items-center gap-1.5"
            onSubmit={(e) => {
              e.preventDefault();
              void saveTitle();
            }}
          >
            <Input
              value={titleDraft}
              className="h-7 min-w-0 flex-1 text-[14px]"
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
          <>
            <h2 className="min-w-0 truncate font-[family-name:var(--font-display)] font-semibold">
              {props.pull.title as string}
            </h2>
            @if (props.canWrite && pullState === "open") {
              <button
                type="button"
                className="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label="Edit title"
                data-testid="pull-v2-title-edit"
                onClick={startTitleEdit}
              >
                <Pencil aria-hidden size={13} />
              </button>;
            }
          </>;
        }
        {pullStateBadge(pullState, props.pull.draft)}
        <div className="ml-auto flex items-center gap-1">
          @if (mergeChip) {
            <span data-testid="pull-v2-merge-state">
              <Badge
                variant={mergeChip.variant === "amber" ? "outline" : mergeChip.variant}
                className={
                  mergeChip.variant === "amber"
                    ? "border-amber-500/50 bg-amber-500/10 text-amber-600 dark:text-amber-400"
                    : undefined
                }
              >
                {mergeChip.label as string}
              </Badge>
            </span>;
          }
          @if (pendingReviewCount > 0) {
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
            className="inline-flex size-7 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
            aria-label="Copy link"
            data-testid="pull-v2-copy-link"
            onClick={() => void copyLink()}
          >
            @if (copied) {
              <Check aria-hidden size={15} className="text-success" />;
            } @else {
              <Copy aria-hidden size={15} />;
            }
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
            data-testid="pull-v2-panel-toggle"
            onClick={() => setPanel(!panelOpen)}
          >
            <MessageSquare aria-hidden size={15} />
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
      <nav className="flex gap-1 overflow-x-auto" aria-label="Pull request sections">
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
    </div>

    <div className="flex items-start gap-4">
      <div className="min-w-0 flex-1 space-y-4">
        <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          <span className="font-medium text-foreground">
            {props.pull.author_username as string}
          </span>
          <span className="inline-flex flex-wrap items-center gap-1.5">
            wants to merge
            @if (commitCount !== undefined) {
              <span>{`${commitCount} commit${commitCount === 1 ? "" : "s"}` as string}</span>;
            }
            into <BranchChip label="base" name={props.pull.base_ref} testid="pull-v2-base-branch" />
            from <BranchChip label="head" name={headRef} testid="pull-v2-head-branch" />
          </span>
          @if (filesQ.data) {
            <span className="ml-auto font-mono text-[13px]" data-testid="pull-v2-diffstat">
              <span className="text-success">{`+${stats.adds}` as string}</span>{" "}
              <span className="text-destructive">{`−${stats.dels}` as string}</span>
            </span>;
          }
        </div>

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
            panelOpen={panelOpen}
            onPanelToggle={setPanel}
          />;
        }
      </div>

      @if (panelOpen && props.tab !== "changes") {
        <aside className="sticky top-36 hidden max-h-[calc(100vh-10rem)] w-80 shrink-0 self-start overflow-hidden rounded-md border border-border bg-card lg:block">
          <PullV2CommentsPanel
            comments={comments}
            currentUserId={props.currentUserId}
            owner={props.owner}
            repoName={props.repoName}
            number={props.number}
            canInteract={props.canInteract}
            onClose={() => setPanel(false)}
            onJump={scrollToFile}
          />
        </aside>;
      }
    </div>
  </div>;
}
