import { useState } from "octane";
import { DiffFileHeader, DiffLargeGate } from "@/components/repo/diff-file-header";
import { DIFF_LARGE_PATCH_CHARS } from "@/lib/diff-budget";
import type { HighlightedDiffLine } from "@/lib/highlight-diff";
import { useLazyDiffRows } from "@/lib/lazy-diff-rows.tsrx";
import type { DiffLineKind } from "@/lib/parse-unified-diff";
import { cn } from "@/lib/utils";

type DiffPatchProps = {
  path: string;
  status: string;
  patch: string;
  /** Pre-highlighted rows from the loader (first paint). */
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

/**
 * Unified-diff file block (commit / compare / PR files, issue #110):
 * collapsible header + per-file "Load diff" gate for large patches + rows
 * highlighted lazily when the block nears the viewport.
 */
export function DiffPatch({ path, status, patch, initialRows = null }: DiffPatchProps) @{
  const large = patch.length > DIFF_LARGE_PATCH_CHARS;
  const [loadLarge, setLoadLarge] = useState(false);
  const [expanded, setExpanded] = useState(true);
  const gated = large && !loadLarge;
  const bodyOpen = expanded && !gated;
  const { rows, rootRef } = useLazyDiffRows({
    patch,
    path,
    open: bodyOpen,
    deferPlainRows: large,
    initialRows,
  });

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

  <section ref={rootRef} className="overflow-hidden border border-border">
    <DiffFileHeader
      path={path}
      status={status}
      patch={patch}
      open={bodyOpen}
      onToggle={() => setExpanded(!expanded)}
    />
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
          @if (patch.trim().length === 0) {
            <p className="px-3 py-3 text-[14px] text-muted-foreground">
              No textual patch for this file.
            </p>;
          } @else {
            <p className="px-3 py-3 text-[14px] text-muted-foreground">Rendering diff…</p>;
          }
        } @else {
          <div
            className="diff-code overflow-x-auto font-mono text-[14px] leading-[1.45]"
            role="table"
            aria-label={`Diff for ${path}`}
          >
            @for (const line of keyed; key line.index) {
              <div
                className={cn(
                  "flex whitespace-pre py-0.5",
                  lineClass(line.kind) as string,
                )}
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
                  <span
                    aria-hidden="true"
                    className="shrink-0 select-none pl-3"
                  />;
                }
                <span
                  className="min-w-0 grow pr-3"
                  dangerouslySetInnerHTML={{ __html: line.contentHtml as string }}
                />
              </div>;
            }
          </div>;
        }
      }
    }
  </section>;
}
