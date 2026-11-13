import { useEffect, useId, useMemo, useRef, useState } from "octane";
import type { PullCommentPublic } from "@oxidean/api-client";
import { MessageSquarePlus } from "@octanejs/lucide";
import { DiffFileHeader, DiffLargeGate } from "@/components/repo/diff-file-header";
import { DIFF_LARGE_PATCH_CHARS } from "@/lib/diff-budget";
import type { HighlightedDiffLine } from "@/lib/highlight-diff";
import { useLazyDiffRows } from "@/lib/lazy-diff-rows.tsrx";
import type { DiffLineKind } from "@/lib/parse-unified-diff";
import { formatRelative } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import {
  anchorLineForSide,
  commentCoversRow,
  commentRangeLabel,
} from "@/components/repo/pull-v2/pull-v2-diff-utils";

/** A gutter-anchored comment target produced by the diff drag/click UI. */
export type DiffCommentTarget = {
  side: "LEFT" | "RIGHT" | null;
  line: number | null;
  /** Range start (`start_line`) when picked via a gutter drag. */
  startLine?: number | null;
};

type DiffPatchProps = {
  path: string;
  status: string;
  patch: string;
  /** Pre-highlighted rows from the loader (first paint). */
  initialRows?: HighlightedDiffLine[] | null;
  /** PR review comments anchored to this file — renders inline when given. */
  comments?: PullCommentPublic[];
  /** Verified-session gate for the "+ comment" affordance + drafts. */
  canInteract?: boolean;
  /** Posts the draft — resolve `true` to close the composer. */
  onSubmitComment?: (target: DiffCommentTarget & { body: string }) => Promise<boolean>;
  /** Discards a pending review draft (`comments.delete`). */
  onDiscardComment?: (id: string) => void;
};

type KeyedRow = HighlightedDiffLine & {
  index: number;
  oldNo: number | null;
  newNo: number | null;
};

type LineAnchor = { side: "LEFT" | "RIGHT"; line: number };

type DragSelection = {
  side: "LEFT" | "RIGHT";
  startLine: number;
  startRow: number;
  endLine: number;
  endRow: number;
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

/** Unified anchor — dels LEFT, ctx/adds RIGHT (same rule as pull-v2). */
function unifiedAnchor(row: KeyedRow): LineAnchor | null {
  if (row.kind === "del") {
    return row.oldNo != null ? { side: "LEFT", line: row.oldNo } : null;
  }
  if (row.kind === "add" || row.kind === "ctx") {
    return row.newNo != null ? { side: "RIGHT", line: row.newNo } : null;
  }
  return null;
}

function anchorForSide(row: KeyedRow, side: "LEFT" | "RIGHT"): LineAnchor | null {
  const line = anchorLineForSide(row.kind, row.oldNo, row.newNo, side);
  return line == null ? null : { side, line };
}

/** A submitted / pending review comment under its anchor line. */
function LegacyCommentNote(props: {
  comment: PullCommentPublic;
  canInteract: boolean;
  onDiscard: ((id: string) => void) | null;
}) @{
  const c = props.comment;
  const rangeLabel = commentRangeLabel(c);
  <div
    className={
      cn(
        "border-t px-8 py-1.5 text-[13px]",
        c.pending ? "border-primary/20 bg-primary/5" : "border-border/50 bg-muted/20",
      ) as string
    }
    data-testid={c.pending ? "pull-pending-comment" : "pull-line-comment"}
  >
    <div className="flex items-center gap-2">
      <span className="font-semibold text-foreground">{`@${c.author_username}` as string}</span>
      <span className="text-[12px] text-muted-foreground">
        {formatRelative(c.created_at) as string}
      </span>
      @if (c.pending) {
        <span className="rounded bg-primary/15 px-1.5 py-0.5 text-[11px] font-semibold uppercase tracking-wide text-primary">
          Pending
        </span>;
      }
      @if (rangeLabel) {
        <span className="text-[12px] text-muted-foreground">{rangeLabel as string}</span>;
      }
      @if (c.resolved) {
        <span className="text-[11px] uppercase tracking-wide text-muted-foreground">
          Resolved
        </span>;
      }
      @if (c.outdated) {
        <span className="text-[11px] uppercase tracking-wide text-muted-foreground">
          Outdated
        </span>;
      }
      @if (c.pending && props.canInteract && props.onDiscard) {
        <button
          type="button"
          className="ml-auto text-[12px] text-muted-foreground underline-offset-2 hover:underline"
          data-testid="pull-pending-discard"
          onClick={() => props.onDiscard?.(c.id)}
        >
          Discard
        </button>;
      }
    </div>
    <p className="mt-1 whitespace-pre-wrap text-foreground">{c.body as string}</p>
  </div>;
}

/** Inline composer opened by the "+ comment" button or a gutter drag. */
function LegacyDraftBox(props: {
  body: string;
  busy: boolean;
  rangeLabel: string | null;
  onBody: (next: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
}) @{
  <div className="border-t border-border/50 bg-muted/20 px-8 py-2" data-testid="pull-line-draft">
    @if (props.rangeLabel) {
      <p className="mb-1 text-[12px] text-muted-foreground">{props.rangeLabel as string}</p>;
    }
    <textarea
      rows={3}
      className="w-full rounded-md border border-border bg-background px-2 py-1.5 font-sans text-[13px] outline-none focus-visible:border-ring"
      placeholder="Leave a comment"
      value={props.body}
      disabled={props.busy}
      onInput={(e) => props.onBody((e.currentTarget as HTMLTextAreaElement).value)}
    />
    <div className="mt-2 flex gap-2">
      <button
        type="button"
        className="rounded-md bg-primary px-2 py-1 text-[13px] font-medium text-primary-foreground disabled:opacity-60"
        disabled={props.busy || !props.body.trim()}
        data-testid="pull-line-draft-add"
        onClick={() => props.onSubmit()}
      >
        Add comment
      </button>
      <button
        type="button"
        className="rounded-md px-2 py-1 text-[13px] text-muted-foreground hover:bg-muted"
        disabled={props.busy}
        onClick={() => props.onCancel()}
      >
        Cancel
      </button>
    </div>
  </div>;
}

/**
 * Unified-diff file block (commit / compare / PR files, issue #110):
 * collapsible header + per-file "Load diff" gate for large patches + rows
 * highlighted lazily when the block nears the viewport.
 *
 * #262 — when `onSubmitComment` is provided (PR files tab), the gutter gets
 * the same "+ comment" button and press-and-drag range selection as the v2
 * viewer, and `comments` render inline with [start_line..line] highlights.
 */
export function DiffPatch({
  path,
  status,
  patch,
  initialRows = null,
  comments,
  canInteract = false,
  onSubmitComment,
  onDiscardComment,
}: DiffPatchProps) @{
  const large = patch.length > DIFF_LARGE_PATCH_CHARS;
  const bodyId = useId();
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

  const commentable = canInteract && !!onSubmitComment;
  const [draft, setDraft] = useState<(DiffCommentTarget & { row: number | null }) | null>(null);
  const [draftBody, setDraftBody] = useState("");
  const [draftBusy, setDraftBusy] = useState(false);
  const [dragSel, setDragSel, getDragSel] = useState<DragSelection | null>(null);

  // Old/new line numbers from hunk headers — GitHub's two gutter columns.
  // O(rows) — memoize so expand/collapse toggles don't re-derive row keys.
  const keyed = useMemo(() => {
    let oldN = 0;
    let newN = 0;
    return rows.map((line, index): KeyedRow => {
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
  }, [rows]);

  const commentsByRow = useMemo(() => {
    const m = new Map<number, PullCommentPublic[]>();
    for (const c of comments ?? []) {
      if (c.line == null) continue;
      for (const r of keyed) {
        const match =
          c.side === "LEFT"
            ? r.oldNo === c.line && (r.kind === "del" || r.kind === "ctx")
            : r.newNo === c.line && (r.kind === "add" || r.kind === "ctx");
        if (match) {
          const list = m.get(r.index) ?? [];
          list.push(c);
          m.set(r.index, list);
          break;
        }
      }
    }
    return m;
  }, [keyed, comments]);

  const coveredRows = useMemo(() => {
    const s = new Set<number>();
    for (const c of comments ?? []) {
      if (c.start_line == null) continue;
      for (const r of keyed) {
        if (commentCoversRow(c, r.kind, r.oldNo, r.newNo)) s.add(r.index);
      }
    }
    return s;
  }, [keyed, comments]);

  const fileComments = useMemo(() => (comments ?? []).filter((c) => c.line == null), [comments]);

  // Gutter drag (#262): mousedown anchors the range and registers the
  // document listeners synchronously — a same-burst mouseup must still land
  // (a useEffect round-trip would miss it). Escape / window blur cancels.
  const dragCleanup = useRef<(() => void) | null>(null);

  function dropDragListeners() {
    dragCleanup.current?.();
    dragCleanup.current = null;
  }

  function cancelDrag() {
    dropDragListeners();
    setDragSel(null);
  }

  function beginDrag(e: MouseEvent, anchor: LineAnchor | null, rowIndex: number) {
    if (!anchor || !commentable || e.button !== 0) return;
    e.preventDefault();
    dropDragListeners();
    setDragSel({
      side: anchor.side,
      startLine: anchor.line,
      startRow: rowIndex,
      endLine: anchor.line,
      endRow: rowIndex,
    });
    const onUp = () => finishDrag();
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") cancelDrag();
    };
    const onBlur = () => cancelDrag();
    document.addEventListener("mouseup", onUp);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onBlur);
    dragCleanup.current = () => {
      document.removeEventListener("mouseup", onUp);
      document.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
    };
  }

  useEffect(() => () => dropDragListeners(), []);

  function hoverRow(row: KeyedRow) {
    const d = getDragSel();
    if (!d) return;
    const a = anchorForSide(row, d.side);
    if (!a || (a.line === d.endLine && row.index === d.endRow)) return;
    setDragSel({ ...d, endLine: a.line, endRow: row.index });
  }

  function finishDrag() {
    const d = getDragSel();
    dropDragListeners();
    setDragSel(null);
    if (!d || d.endRow === d.startRow) return;
    const line = Math.max(d.startLine, d.endLine);
    const startLine = Math.min(d.startLine, d.endLine);
    setDraft({
      row: Math.max(d.startRow, d.endRow),
      side: d.side,
      line,
      startLine,
    });
    setDraftBody("");
  }

  function inDragRange(row: KeyedRow): boolean {
    if (!dragSel) return false;
    const lo = Math.min(dragSel.startRow, dragSel.endRow);
    const hi = Math.max(dragSel.startRow, dragSel.endRow);
    if (row.index < lo || row.index > hi) return false;
    return anchorForSide(row, dragSel.side) != null;
  }

  async function submitDraft() {
    const t = draft;
    const body = draftBody.trim();
    if (!t || !body || draftBusy || !onSubmitComment) return;
    setDraftBusy(true);
    const ok = await onSubmitComment({
      side: t.side,
      line: t.line,
      startLine: t.startLine ?? null,
      body,
    });
    setDraftBusy(false);
    if (ok) {
      setDraft(null);
      setDraftBody("");
    }
  }

  <section ref={rootRef} className="overflow-hidden border border-border">
    <DiffFileHeader
      path={path}
      status={status}
      patch={patch}
      open={bodyOpen}
      onToggle={() => setExpanded(!expanded)}
      controlsId={bodyId}
      commentCount={(comments ?? []).length}
    />
    @if (draft?.row === null || fileComments.length > 0) {
      <div className="border-b border-border/50">
        @if (draft?.row === null) {
          <LegacyDraftBox
            body={draftBody}
            busy={draftBusy}
            rangeLabel={null}
            onBody={setDraftBody}
            onSubmit={() => void submitDraft()}
            onCancel={() => setDraft(null)}
          />;
        }
        @for (const c of fileComments; key c.id) {
          <LegacyCommentNote
            comment={c}
            canInteract={canInteract}
            onDiscard={c.pending ? (onDiscardComment ?? null) : null}
          />;
        }
      </div>;
    }
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
            @if (patch.trim().length === 0) {
              <p className="px-3 py-3 text-[14px] text-muted-foreground">
                No textual patch for this file.
              </p>;
            } @else {
              <p className="px-3 py-3 text-[14px] text-muted-foreground">Rendering diff…</p>;
            }
          } @else {
            <div
              className={
                cn(
                  "diff-code overflow-x-auto font-mono text-[14px] leading-[1.45]",
                  dragSel && "select-none",
                ) as string
              }
              role="table"
              aria-label={`Diff for ${path}`}
            >
              @for (const line of keyed; key line.index) {
                <div
                  className="group"
                  data-diff-row={line.index as number}
                  onMouseOver={() => hoverRow(line)}
                >
                  <div
                    className={
                      cn(
                        "flex whitespace-pre py-0.5",
                        lineClass(line.kind) as string,
                        inDragRange(line) &&
                          "outline-1 -outline-offset-1 outline-primary/60",
                        coveredRows.has(line.index) &&
                          "shadow-[inset_2px_0_0_0_var(--primary)]",
                      ) as string
                    }
                    data-diff-kind={line.kind as string}
                    role="row"
                  >
                    @if (commentable) {
                      <span
                        className="w-6 shrink-0 select-none pl-1"
                        data-diff-gutter
                        onMouseDown={(e) =>
                          beginDrag(e, unifiedAnchor(line), line.index)
                        }
                      >
                        @if (unifiedAnchor(line)) {
                          <button
                            type="button"
                            className="inline-flex size-4 items-center justify-center rounded-[3px] bg-primary text-primary-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
                            aria-label={`Comment on line ${unifiedAnchor(line)?.line ?? ""}`}
                            data-testid="pull-line-comment-add"
                            onClick={() => {
                              const a = unifiedAnchor(line);
                              if (!a) return;
                              setDraft({
                                row: line.index,
                                side: a.side,
                                line: a.line,
                                startLine: null,
                              });
                              setDraftBody("");
                            }}
                          >
                            <MessageSquarePlus aria-hidden size={10} />
                          </button>;
                        } @else {
                          <span className="inline-block size-4" />;
                        }
                      </span>;
                    }
                    <span
                      aria-hidden="true"
                      className={
                        cn(
                          "w-10 shrink-0 select-none pr-2 text-right",
                          inDragRange(line) || coveredRows.has(line.index)
                            ? "bg-primary/10 text-primary"
                            : "text-muted-foreground/60",
                        ) as string
                      }
                      data-diff-gutter={commentable ? true : undefined}
                      onMouseDown={
                        commentable
                          ? (e) => beginDrag(e, unifiedAnchor(line), line.index)
                          : undefined
                      }
                    >
                      {(line.oldNo ?? "") as number | string}
                    </span>
                    <span
                      aria-hidden="true"
                      className={
                        cn(
                          "w-10 shrink-0 select-none border-r border-border/60 pr-2 text-right",
                          inDragRange(line) || coveredRows.has(line.index)
                            ? "bg-primary/10 text-primary"
                            : "text-muted-foreground/60",
                        ) as string
                      }
                      data-diff-gutter={commentable ? true : undefined}
                      onMouseDown={
                        commentable
                          ? (e) => beginDrag(e, unifiedAnchor(line), line.index)
                          : undefined
                      }
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
                  </div>
                  @if (draft?.row === line.index) {
                    <LegacyDraftBox
                      body={draftBody}
                      busy={draftBusy}
                      rangeLabel={
                        draft.startLine != null && draft.line != null
                          ? `Commenting on lines ${draft.startLine}–${draft.line}`
                          : null
                      }
                      onBody={setDraftBody}
                      onSubmit={() => void submitDraft()}
                      onCancel={() => setDraft(null)}
                    />;
                  }
                  @for (const c of commentsByRow.get(line.index) ??
                      []; key c.id) {
                    <LegacyCommentNote
                      comment={c}
                      canInteract={canInteract}
                      onDiscard={c.pending ? (onDiscardComment ?? null) : null}
                    />;
                  }
                </div>;
              }
            </div>;
          }
        }
      }
    </div>
  </section>;
}
