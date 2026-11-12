import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type {
  PullCommentPublic,
  PullDiffFile,
  PullFilesRequest,
  PullPublic,
} from "@oxidean/api-client";
import { useId, useMemo, useState } from "octane";
import { DiffFileHeader, DiffLargeGate } from "@/components/repo/diff-file-header";
import { DiffPatch, type DiffCommentTarget } from "@/components/repo/diff-patch";
import { AppLink } from "@/components/ui/app-link";
import { apiClient } from "@/lib/api-client";
import { DIFF_FILES_PAGE_SIZE, DIFF_LARGE_PATCH_CHARS } from "@/lib/diff-budget";
import { useIncrementalList } from "@/lib/incremental-list.tsrx";
import { useLazyDiffRows } from "@/lib/lazy-diff-rows.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";
import { matchPath } from "@/lib/route-params";
import { cn } from "@/lib/utils";
import {
  changesScopeKey,
  parseChangesScope,
  scopeAnchorSha,
} from "@/components/repo/pull-v2/pull-v2-diff-utils";

type PullFilesProps = {
  owner: string;
  repoName: string;
  number: number;
  canInteract: boolean;
  /** Scoped diff (`<sha>` | `<base>..<head>`); defaults to the URL splat. */
  changesSha?: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function splitLineClass(kind: string): string {
  if (kind === "add") return "bg-[var(--diff-add-bg)]";
  if (kind === "del") return "bg-[var(--diff-del-bg)]";
  return "";
}

/**
 * Split-view diff file block (issue #110): same collapse / large-diff gate /
 * lazy-highlight behavior as {@link DiffPatch} — only the row layout differs.
 * Read-only: inline comments live on the unified view (#262).
 */
function SplitDiff(props: {
  path: string;
  status: string;
  patch: string;
  commentCount?: number;
}) @{
  const large = props.patch.length > DIFF_LARGE_PATCH_CHARS;
  const bodyId = useId();
  const [loadLarge, setLoadLarge] = useState(false);
  const [expanded, setExpanded] = useState(true);
  const gated = large && !loadLarge;
  const bodyOpen = expanded && !gated;
  const { rows, rootRef } = useLazyDiffRows({
    patch: props.patch,
    path: props.path,
    open: bodyOpen,
    deferPlainRows: large,
  });

  // O(rows) — memoize so expand/collapse toggles don't re-derive row keys.
  const keyed = useMemo(() => rows.map((line, index) => ({ ...line, index })), [rows]);

  <section ref={rootRef} className="overflow-hidden border border-border">
    <DiffFileHeader
      path={props.path}
      status={props.status}
      patch={props.patch}
      open={bodyOpen}
      onToggle={() => setExpanded(!expanded)}
      controlsId={bodyId}
      commentCount={props.commentCount ?? 0}
    />
    <div id={bodyId}>
      @if (gated) {
        <DiffLargeGate
          onLoad={() => {
            setLoadLarge(true);
            setExpanded(true);
          }}
        />;
      } @else {
        @if (expanded) {
          @if (keyed.length === 0) {
            @if (props.patch.trim().length === 0) {
              <p className="px-3 py-3 text-[14px] text-muted-foreground">
                No textual patch for this file.
              </p>;
            } @else {
              <p className="px-3 py-3 text-[14px] text-muted-foreground">Rendering diff…</p>;
            }
          } @else {
            <div className="diff-code overflow-x-auto font-mono text-[14px] leading-[1.45]">
              @for (const line of keyed; key line.index) {
                <div
                  className={cn(
                    "grid grid-cols-2 whitespace-pre border-b border-border/40",
                    splitLineClass(line.kind) as string,
                  )}
                  data-diff-kind={line.kind as string}
                >
                  <div className="flex border-r border-border/50 px-2 py-0.5 text-muted-foreground">
                    @if (line.kind !== "add") {
                      <>
                        @if (line.prefix) {
                          <span
                            className="shrink-0 select-none"
                            aria-hidden="true"
                          >
                            {line.prefix as string}
                          </span>;
                        }
                        <span
                          className="min-w-0 grow"
                          dangerouslySetInnerHTML={{
                            __html: line.contentHtml as string,
                          }}
                        />
                      </>;
                    }
                  </div>
                  <div className="flex px-2 py-0.5">
                    @if (line.kind !== "del") {
                      <>
                        @if (line.prefix) {
                          <span
                            className="shrink-0 select-none"
                            aria-hidden="true"
                          >
                            {line.prefix as string}
                          </span>;
                        }
                        <span
                          className="min-w-0 grow"
                          dangerouslySetInnerHTML={{
                            __html: line.contentHtml as string,
                          }}
                        />
                      </>;
                    }
                  </div>
                </div>;
              }
            </div>;
          }
        }
      }
    </div>
  </section>;
}

/**
 * Legacy files-changed tab. The commit scope rides the URL splat
 * (`/pull/N/changes/<sha>` or `/pull/N/changes/<base>..<head>`) so the
 * per-commit "changes" links on the commits tab — and reloads — land on the
 * same scoped diff (#261). Inline comments post immediately and can be
 * dragged across a contiguous line range (#262).
 */
export function PullFiles(props: PullFilesProps) @{
  const queryClient = useQueryClient();
  const [view, setView] = useState<"unified" | "split">("unified");
  const [error, setError] = useState<string | null>(null);

  const pathname = usePathname();
  const scope = useMemo(() => {
    if (props.changesSha !== undefined) return parseChangesScope(props.changesSha);
    const m = matchPath("/$owner/$repo/pull/$n/$", pathname);
    const splat = (m?._splat ?? "") as string;
    if (splat.startsWith("changes/")) return parseChangesScope(splat.slice(8));
    if (splat.startsWith("files/")) return parseChangesScope(splat.slice(6));
    return parseChangesScope(undefined);
  }, [pathname, props.changesSha]);
  const scopeKey = changesScopeKey(scope);
  const base = `/${props.owner}/${props.repoName}/pull/${props.number}`;

  const filesQ = useQuery({
    queryKey: ["pull", "files", props.owner, props.repoName, props.number, scopeKey] as const,
    queryFn: async () => {
      const input: PullFilesRequest = {
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      };
      if (scope.kind === "sha") input.sha = scope.sha;
      else if (scope.kind === "range") {
        input.base_sha = scope.base;
        input.head_sha = scope.head;
      }
      const res = await apiClient.pull.files(input);
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  // Same key the page uses — cache hit; gives the comment anchor head_sha.
  const pullQ = useQuery({
    queryKey: ["pull", "get", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.get({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });
  const pull: PullPublic | undefined = pullQ.data;

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
  const allComments: PullCommentPublic[] = commentsQ.data?.comments ?? [];
  const commentsByPath = useMemo(() => {
    const m: Record<string, PullCommentPublic[]> = {};
    for (const c of allComments) {
      if (!c.path) continue;
      (m[c.path] ??= []).push(c);
    }
    return m;
  }, [allComments]);
  const commentCounts = useMemo(() => {
    const m: Record<string, number> = {};
    for (const c of allComments) {
      if (!c.path) continue;
      m[c.path] = (m[c.path] ?? 0) + 1;
    }
    return m;
  }, [allComments]);

  const list = useIncrementalList(
    files.length,
    DIFF_FILES_PAGE_SIZE,
    `${props.owner}/${props.repoName}#${props.number}|${scopeKey}`,
  );
  const shown = files.slice(0, list.count);
  const anchorSha = scopeAnchorSha(scope, pull?.head_sha ?? undefined);

  async function submitComment(t: DiffCommentTarget & { path: string; body: string }) {
    const res = await apiClient.pull.comments.create({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      body: t.body,
      path: t.path,
      side: t.side,
      line: t.line,
      start_line: t.startLine ?? null,
      commit_sha: anchorSha,
    });
    if (!res.ok) {
      setError(res.error.message || "Could not post comment");
      return false;
    }
    setError(null);
    await queryClient.invalidateQueries({
      queryKey: ["pull", "comments", props.owner, props.repoName, props.number],
    });
    return true;
  }

  async function discardComment(id: string) {
    const res = await apiClient.pull.comments.delete({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      commentId: id,
    });
    if (res.ok) {
      await queryClient.invalidateQueries({
        queryKey: ["pull", "comments", props.owner, props.repoName, props.number],
      });
    } else {
      setError(res.error.message || "Could not discard comment");
    }
  }

  <div className="space-y-3" data-testid="pull-files">
    @if (scope.kind === "sha" || scope.kind === "range") {
      <div
        className="flex flex-wrap items-center gap-2 rounded-md border border-primary/30 bg-primary/5 px-3 py-2 text-[13px] text-foreground"
        data-testid="pull-files-scope-notice"
      >
        @if (scope.kind === "sha") {
          <span>
            Viewing changes from commit{" "}
            <span className="font-mono">{scope.sha.slice(0, 7) as string}</span>
          </span>;
        } @else {
          <span>
            Viewing changes in range{" "}
            <span className="font-mono">
              {`${scope.base.slice(0, 7)}..${scope.head.slice(0, 7)}` as string}
            </span>
          </span>;
        }
        <AppLink
          href={`${base}/changes` as string}
          className="font-medium text-primary no-underline hover:underline"
          data-testid="pull-files-scope-show-all"
        >
          Show all
        </AppLink>
      </div>;
    }
    <div className="flex items-center gap-2" role="group" aria-label="Diff view">
      <button
        type="button"
        className={
          (view === "unified"
            ? "rounded-md bg-muted px-2 py-1 text-sm font-medium"
            : "rounded-md px-2 py-1 text-sm text-muted-foreground") as string
        }
        onClick={() => setView("unified")}
      >
        Unified
      </button>
      <button
        type="button"
        className={
          (view === "split"
            ? "rounded-md bg-muted px-2 py-1 text-sm font-medium"
            : "rounded-md px-2 py-1 text-sm text-muted-foreground") as string
        }
        onClick={() => setView("split")}
      >
        Split
      </button>
    </div>

    @if (filesQ.isPending) {
      <p className="text-sm text-muted-foreground">Loading files…</p>;
    } @else {
      @if (filesQ.isError) {
        <p className="text-sm text-destructive">Could not load diff.</p>;
      } @else {
        @if (files.length === 0) {
          <p className="text-sm text-muted-foreground">No file changes.</p>;
        } @else {
          <>
            <div className="space-y-4">
              @for (const f of shown; key f.path) {
                @if (view === "unified") {
                  <DiffPatch
                    path={f.path}
                    status={f.status}
                    patch={f.patch}
                    comments={commentsByPath[f.path]}
                    canInteract={props.canInteract}
                    onSubmitComment={(t) => submitComment({ ...t, path: f.path })}
                    onDiscardComment={(id) => void discardComment(id)}
                  />;
                } @else {
                  <SplitDiff
                    path={f.path}
                    status={f.status}
                    patch={f.patch}
                    commentCount={commentCounts[f.path] ?? 0}
                  />;
                }
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
    @if (error) {
      <p className="text-sm text-destructive">{error as string}</p>;
    }
  </div>;
}
