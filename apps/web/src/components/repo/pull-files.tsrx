import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { PullDiffFile } from "@oxidean/api-client";
import { useId, useMemo, useState } from "octane";
import { DiffFileHeader, DiffLargeGate } from "@/components/repo/diff-file-header";
import { DiffPatch } from "@/components/repo/diff-patch";
import { apiClient } from "@/lib/api-client";
import { DIFF_FILES_PAGE_SIZE, DIFF_LARGE_PATCH_CHARS } from "@/lib/diff-budget";
import { useIncrementalList } from "@/lib/incremental-list.tsrx";
import { useLazyDiffRows } from "@/lib/lazy-diff-rows.tsrx";
import { cn } from "@/lib/utils";

type PullFilesProps = {
  owner: string;
  repoName: string;
  number: number;
  canInteract: boolean;
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
 */
function SplitDiff(props: { path: string; status: string; patch: string }) @{
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

export function PullFiles(props: PullFilesProps) @{
  const queryClient = useQueryClient();
  const [view, setView] = useState<"unified" | "split">("unified");
  const [linePath, setLinePath] = useState<string | null>(null);
  const [lineDraft, setLineDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  const files: PullDiffFile[] = filesQ.data?.files ?? [];
  const list = useIncrementalList(
    files.length,
    DIFF_FILES_PAGE_SIZE,
    `${props.owner}/${props.repoName}#${props.number}`,
  );
  const shown = files.slice(0, list.count);

  async function postLineComment(path: string) {
    if (!lineDraft.trim()) return;
    setBusy(true);
    setError(null);
    const res = await apiClient.pull.comments.create({
      owner: props.owner,
      name: props.repoName,
      number: props.number,
      body: lineDraft,
      path,
      side: "RIGHT",
      line: 1,
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not post line comment");
      return;
    }
    setLineDraft("");
    setLinePath(null);
    await queryClient.invalidateQueries({
      queryKey: ["pull", "comments", props.owner, props.repoName, props.number],
    });
  }

  <div className="space-y-3" data-testid="pull-files">
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
                <div>
                  @if (view === "unified") {
                    <DiffPatch path={f.path} status={f.status} patch={f.patch} />;
                  } @else {
                    <SplitDiff path={f.path} status={f.status} patch={f.patch} />;
                  }
                  @if (props.canInteract) {
                    <div className="mt-1 px-1">
                      @if (linePath === f.path) {
                        <div className="flex flex-wrap items-center gap-2">
                          <input
                            className="min-w-[12rem] flex-1 rounded-md border border-border bg-background px-2 py-1 text-sm"
                            value={lineDraft}
                            placeholder="Line comment"
                            onInput={(e) =>
                              setLineDraft(
                                (e.currentTarget as HTMLInputElement).value,
                              )
                            }
                          />
                          <button
                            type="button"
                            className="text-sm text-primary"
                            disabled={busy}
                            onClick={() => void postLineComment(f.path)}
                          >
                            Comment
                          </button>
                          <button
                            type="button"
                            className="text-sm text-muted-foreground"
                            onClick={() => {
                              setLinePath(null);
                              setLineDraft("");
                            }}
                          >
                            Cancel
                          </button>
                        </div>;
                      } @else {
                        <button
                          type="button"
                          className="text-[13px] text-muted-foreground underline-offset-2 hover:underline"
                          onClick={() => setLinePath(f.path)}
                        >
                          Add line comment
                        </button>;
                      }
                    </div>;
                  }
                </div>;
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
