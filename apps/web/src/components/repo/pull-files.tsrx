import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { PullDiffFile } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { DiffPatch } from "@/components/repo/diff-patch";
import { apiClient } from "@/lib/api-client";
import { clientHighlightTheme } from "@/lib/highlight";
import {
  highlightDiffLines,
  highlightedDiffRowsEqual,
  plainHighlightedDiffLines,
  type HighlightedDiffLine,
} from "@/lib/highlight-diff";
import { parseUnifiedDiffLines } from "@/lib/parse-unified-diff";
import { cn } from "@/lib/utils";

export type PullFilesProps = {
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

function SplitDiff(props: { path: string; status: string; patch: string }) @{
  const parsed = parseUnifiedDiffLines(props.patch);
  const [rows, setRows] = useState<HighlightedDiffLine[]>(() => plainHighlightedDiffLines(parsed));

  useEffect(() => {
    const lines = parseUnifiedDiffLines(props.patch);
    setRows(plainHighlightedDiffLines(lines));

    let cancelled = false;
    let appliedTheme: ReturnType<typeof clientHighlightTheme> | null = null;

    async function run() {
      const theme = clientHighlightTheme();
      if (appliedTheme === theme) return;
      const next = await highlightDiffLines(lines, {
        path: props.path,
        theme,
      });
      if (cancelled) return;
      appliedTheme = theme;
      setRows((prev) => (highlightedDiffRowsEqual(prev, next) ? prev : next));
    }

    void run();

    if (typeof document === "undefined") {
      return () => {
        cancelled = true;
      };
    }

    const mo = new MutationObserver(() => {
      void run();
    });
    mo.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    return () => {
      cancelled = true;
      mo.disconnect();
    };
  }, [props.patch, props.path]);

  const keyed = rows.map((line, index) => ({ ...line, index }));

  <section className="overflow-hidden border border-border">
    <h3 className="border-b border-border bg-muted/40 px-3 py-2 font-mono text-[14px]">
      {props.path as string}{" "}
      <span className="text-muted-foreground">({props.status as string})</span>
    </h3>
    @if (keyed.length === 0) {
      <p className="px-3 py-3 text-[14px] text-muted-foreground">
        No textual patch for this file.
      </p>;
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
          <div className="space-y-4">
            @for (const f of files; key f.path) {
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
          </div>;
        }
      }
    }
    @if (error) {
      <p className="text-sm text-destructive">{error as string}</p>;
    }
  </div>;
}
