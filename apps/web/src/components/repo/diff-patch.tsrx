import { useEffect, useState } from "octane";
import type { DiffLineKind } from "@/lib/parse-unified-diff";
import { parseUnifiedDiffLines } from "@/lib/parse-unified-diff";
import {
  highlightDiffLines,
  highlightedDiffRowsEqual,
  plainHighlightedDiffLines,
  type HighlightedDiffLine,
} from "@/lib/highlight-diff";
import { cn } from "@/lib/utils";

type DiffPatchProps = {
  path: string;
  status: string;
  patch: string;
  /** Pre-highlighted rows from SSR loader (first paint). */
  initialRows?: HighlightedDiffLine[] | null;
};

function lineClass(kind: DiffLineKind): string {
  switch (kind) {
    case "add":
      return "bg-[var(--diff-add-bg)]";
    case "del":
      return "bg-[var(--diff-del-bg)]";
    case "hunk":
      return "bg-[var(--diff-hunk-bg)] text-[var(--diff-hunk-fg)]";
    case "meta":
    case "note":
      return "text-muted-foreground";
    case "ctx":
    default:
      return "text-foreground";
  }
}

function prefixClass(kind: DiffLineKind): string {
  switch (kind) {
    case "add":
      return "text-[var(--diff-add-fg)]";
    case "del":
      return "text-[var(--diff-del-fg)]";
    default:
      return "text-muted-foreground";
  }
}

function initialRowsOrPlain(
  patch: string,
  initialRows: HighlightedDiffLine[] | null | undefined,
): HighlightedDiffLine[] {
  if (initialRows && initialRows.length > 0) return initialRows;
  return plainHighlightedDiffLines(parseUnifiedDiffLines(patch));
}

/** Unified-diff file block: path header + colorized hunk lines (commit / compare / PR). */
export function DiffPatch({ path, status, patch, initialRows = null }: DiffPatchProps) @{
  const [rows, setRows] = useState<HighlightedDiffLine[]>(() =>
    initialRowsOrPlain(patch, initialRows),
  );

  useEffect(() => {
    const lines = parseUnifiedDiffLines(patch);
    let cancelled = false;

    // SSR rows are dual-theme (`light-dark()`) — already correct for either
    // scheme; take them as-is and skip the re-highlight pass.
    if (initialRows && initialRows.length > 0) {
      setRows((prev) => (highlightedDiffRowsEqual(prev, initialRows) ? prev : initialRows));
      return () => {
        cancelled = true;
      };
    }

    setRows(plainHighlightedDiffLines(lines));

    async function run() {
      const next = await highlightDiffLines(lines, { path });
      if (cancelled) return;
      setRows((prev) => (highlightedDiffRowsEqual(prev, next) ? prev : next));
    }

    void run();

    return () => {
      cancelled = true;
    };
  }, [patch, path, initialRows]);

  // Old/new line numbers from hunk headers — GitHub's two gutter columns.
  let oldN = 0;
  let newN = 0;
  const keyed = rows.map((line, index) => {
    let oldNo: number | null = null;
    let newNo: number | null = null;
    if (line.kind === "hunk") {
      const m = /@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line.text);
      if (m) {
        oldN = Number(m[1]);
        newN = Number(m[2]);
      }
    } else if (line.kind === "del") {
      oldNo = oldN++;
    } else if (line.kind === "add") {
      newNo = newN++;
    } else if (line.kind === "ctx") {
      oldNo = oldN++;
      newNo = newN++;
    }
    return { ...line, index, oldNo, newNo };
  });

  <section className="overflow-hidden border border-border">
    <h3 className="border-b border-border bg-muted/40 px-3 py-2 font-mono text-[14px] text-foreground">
      {path as string} <span className="text-muted-foreground">({status as string})</span>
    </h3>
    @if (keyed.length === 0) {
      <p className="px-3 py-3 text-[14px] text-muted-foreground">
        No textual patch for this file.
      </p>;
    } @else {
      <div
        className="diff-code overflow-x-auto font-mono text-[14px] leading-[1.45]"
        role="table"
        aria-label={`Diff for ${path}`}
      >
        @for (const line of keyed; key line.index) {
          <div
            className={cn("flex whitespace-pre py-0.5", lineClass(line.kind) as string)}
            role="row"
            data-diff-kind={line.kind as string}
          >
            <span
              aria-hidden="true"
              className="w-10 shrink-0 select-none pr-2 text-right text-muted-foreground/60"
            >
              {(line.oldNo ?? "") as number | string}
            </span>
            <span
              aria-hidden="true"
              className="w-10 shrink-0 select-none border-r border-border/60 pr-2 text-right text-muted-foreground/60"
            >
              {(line.newNo ?? "") as number | string}
            </span>
            @if (line.prefix) {
              <span
                className={cn(
                  "shrink-0 select-none pl-3",
                  prefixClass(line.kind) as string,
                )}
                aria-hidden="true"
              >
                {line.prefix as string}
              </span>;
            } @else {
              <span aria-hidden="true" className="shrink-0 select-none pl-3" />;
            }
            <span
              className="min-w-0 grow pr-3"
              dangerouslySetInnerHTML={{ __html: line.contentHtml as string }}
            />
          </div>;
        }
      </div>;
    }
  </section>;
}
