import type { PullCommentPublic, PullDiffFile } from "@oxidean/api-client";
import {
  Check,
  ChevronDown,
  ChevronRight,
  Ellipsis,
  MessageSquare,
  MessageSquarePlus,
} from "@octanejs/lucide";
import { useEffect, useId, useMemo, useRef, useState } from "octane";
import { MarkdownBody } from "@/components/repo/markdown-body";
import { DiffLargeGate } from "@/components/repo/diff-file-header";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLinkItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Textarea } from "@/components/ui/textarea";
import { DIFF_LARGE_PATCH_CHARS } from "@/lib/diff-budget";
import type { HighlightedDiffLine } from "@/lib/highlight-diff";
import { useLazyDiffRows } from "@/lib/lazy-diff-rows.tsrx";
import { diffPatchStatCounts } from "@/lib/parse-unified-diff";
import { formatRelative } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import { ComposerExtras } from "./pull-v2-composer-extras";
import {
  anchorLineForSide,
  commentCoversRow,
  commentRangeLabel,
  diffLineClass,
  fileAnchorId,
  fileStatusMeta,
  type DraftTarget,
} from "./pull-v2-diff-utils";

type KeyedRow = HighlightedDiffLine & {
  index: number;
  oldNo: number | null;
  newNo: number | null;
};

type LineAnchor = { side: "LEFT" | "RIGHT"; line: number };

/** In-progress gutter drag (mouse down on a line-number cell). */
type DragSelection = {
  side: "LEFT" | "RIGHT";
  startLine: number;
  startRow: number;
  endLine: number;
  endRow: number;
};

/** Unified-view anchor for a row: dels anchor LEFT, ctx/adds anchor RIGHT. */
function unifiedAnchor(row: KeyedRow): LineAnchor | null {
  if (row.kind === "del") {
    return row.oldNo != null ? { side: "LEFT", line: row.oldNo } : null;
  }
  if (row.kind === "add" || row.kind === "ctx") {
    return row.newNo != null ? { side: "RIGHT", line: row.newNo } : null;
  }
  return null;
}

/** Split-view anchors — each half cell gets its own side's line number. */
function splitAnchors(row: KeyedRow): { left: LineAnchor | null; right: LineAnchor | null } {
  const left =
    (row.kind === "del" || row.kind === "ctx") && row.oldNo != null
      ? { side: "LEFT" as const, line: row.oldNo }
      : null;
  const right =
    (row.kind === "add" || row.kind === "ctx") && row.newNo != null
      ? { side: "RIGHT" as const, line: row.newNo }
      : null;
  return { left, right };
}

/** Row anchor pinned to one side — the side stays fixed for a whole drag. */
function anchorForSide(row: KeyedRow, side: "LEFT" | "RIGHT"): LineAnchor | null {
  const line = anchorLineForSide(row.kind, row.oldNo, row.newNo, side);
  return line == null ? null : { side, line };
}

function prefixClass(kind: string): string {
  if (kind === "add") return "text-[var(--diff-add-fg)]";
  if (kind === "del") return "text-[var(--diff-del-fg)]";
  return "text-muted-foreground";
}

/**
 * "+ comment" gutter button (WI-0001 S6). Lines that cannot be anchored
 * (hunk/meta/note rows carry no line number) render a disabled button.
 * A plain click opens a single-line draft; press-and-drag on the gutter
 * selects a range instead (handled by the parent row).
 */
function LineCommentButton(props: {
  anchor: LineAnchor | null;
  canInteract: boolean;
  onAdd: (anchor: LineAnchor) => void;
}) @{
  @if (props.anchor && props.canInteract) {
    <button
      type="button"
      className="inline-flex size-4 items-center justify-center rounded-[3px] bg-primary text-primary-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
      aria-label={`Comment on line ${props.anchor.line}`}
      data-testid="pull-v2-line-comment-add"
      onClick={() => props.anchor && props.onAdd(props.anchor)}
    >
      <MessageSquarePlus aria-hidden size={10} />
    </button>;
  } @else {
    <button
      type="button"
      disabled
      aria-hidden="true"
      className="inline-flex size-4 items-center justify-center opacity-0"
      tabIndex={-1}
    >
      <MessageSquarePlus size={10} />
    </button>;
  }
}

/**
 * Inline draft box under the row being commented on. "Add review comment"
 * persists a pending review draft; "Add single comment" posts immediately —
 * the GitHub review-vs-single-comment split (#263, pending drafts).
 */
function LineDraftBox(props: {
  body: string;
  busy: boolean;
  error: string | null;
  /** Write+ — "Add review comment" persists a server-side pending draft. */
  canWrite: boolean;
  /** Range comment header (`start_line..line`) when the draft came from a drag. */
  rangeLabel: string | null;
  onBody: (next: string) => void;
  onSubmit: (pending: boolean) => void;
  onCancel: () => void;
}) @{
  const [uploading, setUploading] = useState(false);
  const busy = props.busy || uploading;
  <div className="border-t border-border/50 bg-muted/20 px-8 py-2" data-testid="pull-v2-line-draft">
    @if (props.rangeLabel) {
      <p className="mb-1 text-[12px] text-muted-foreground">{props.rangeLabel as string}</p>;
    }
    <Textarea
      rows={3}
      className="font-sans text-[13px]"
      placeholder="Leave a review comment"
      value={props.body}
      disabled={busy}
      onInput={(e) => props.onBody((e.currentTarget as HTMLTextAreaElement).value)}
    />
    <div className="mt-2 flex flex-wrap items-center gap-2">
      @if (props.canWrite) {
        <button
          type="button"
          className="rounded-md bg-primary px-2 py-1 text-[13px] font-medium text-primary-foreground disabled:opacity-60"
          disabled={busy || !props.body.trim()}
          data-testid="pull-v2-line-draft-add"
          onClick={() => props.onSubmit(true)}
        >
          Add review comment
        </button>;
      }
      <button
        type="button"
        className="rounded-md border border-border px-2 py-1 text-[13px] text-foreground hover:bg-muted disabled:opacity-60"
        disabled={busy || !props.body.trim()}
        data-testid="pull-v2-line-draft-add-single"
        onClick={() => props.onSubmit(false)}
      >
        Add single comment
      </button>
      <button
        type="button"
        className="rounded-md px-2 py-1 text-[13px] text-muted-foreground hover:bg-muted"
        disabled={busy}
        data-testid="pull-v2-line-draft-cancel"
        onClick={() => props.onCancel()}
      >
        Cancel
      </button>
      <ComposerExtras
        value={props.body}
        onValueChange={props.onBody}
        disabled={busy}
        testidPrefix="pull-v2-line-draft"
        onUploadingChange={setUploading}
      />
    </div>
    @if (props.error) {
      <p role="alert" className="mt-2 text-[12px] text-destructive">
        {props.error as string}
      </p>;
    }
  </div>;
}

/**
 * A review comment rendered under its anchor line — server-persisted pending
 * drafts render like submitted comments plus a "Pending" badge + Discard.
 */
function CommentNote(props: {
  comment: PullCommentPublic;
  owner: string;
  repoName: string;
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
    data-testid={c.pending ? "pull-v2-pending-comment" : "pull-v2-line-comment"}
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
          data-testid="pull-v2-pending-discard"
          onClick={() => props.onDiscard?.(c.id)}
        >
          Discard
        </button>;
      }
    </div>
    <MarkdownBody
      markdown={c.body}
      owner={props.owner}
      repo={props.repoName}
      className="mt-1 text-[13px]"
    />
  </div>;
}

/**
 * Draft box + anchored comments rendered under a diff row (or under the file
 * header for `row: null` file-level notes). `minimizeComments` hides the
 * threads but never the open draft.
 */
function RowNotes(props: {
  owner: string;
  repoName: string;
  draft: DraftTarget | null;
  draftBody: string;
  draftBusy: boolean;
  draftError: string | null;
  comments: PullCommentPublic[];
  canInteract: boolean;
  /** Write+ — hides the pending-draft "Add review comment" path. */
  canWrite: boolean;
  minimizeComments: boolean;
  onDraftBody: (body: string) => void;
  onSubmitDraft: (pending: boolean) => void;
  onCancelDraft: () => void;
  onDiscardPending: (id: string) => void;
}) @{
  <>
    @if (props.draft) {
      <LineDraftBox
        body={props.draftBody}
        busy={props.draftBusy}
        error={props.draftError}
        canWrite={props.canWrite}
        rangeLabel={
          props.draft.startLine != null && props.draft.line != null
            ? `Commenting on lines ${props.draft.startLine}–${props.draft.line}`
            : null
        }
        onBody={props.onDraftBody}
        onSubmit={props.onSubmitDraft}
        onCancel={props.onCancelDraft}
      />;
    }
    @if (!props.minimizeComments) {
      @for (const c of props.comments; key c.id) {
        <CommentNote
          comment={c}
          owner={props.owner}
          repoName={props.repoName}
          canInteract={props.canInteract}
          onDiscard={c.pending ? props.onDiscardPending : null}
        />;
      }
    }
  </>;
}

/**
 * Per-file diff block for the v2 changes tab (WI-0001 S4–S6, #261–#263):
 * lazy highlight + large-diff gate + collapse, viewed marks (unscoped views
 * only), "+ comment"/gutter-drag line anchors writing server-side pending
 * drafts, range comments, file-level comments, and the per-file menu.
 */
export function PullV2DiffFile(props: {
  owner: string;
  repoName: string;
  file: PullDiffFile;
  view: "unified" | "split";
  wrap: boolean;
  compact: boolean;
  expanded: boolean;
  viewed: boolean;
  /** Viewed checkbox/marks allowed — false on scoped (sha/range/since) views. */
  marksEnabled: boolean;
  canInteract: boolean;
  /** Write+ capability — pending review drafts are Write-gated (#263). */
  canWrite?: boolean;
  minimizeComments: boolean;
  /** Server-side pending drafts anchored to this file. */
  pending: PullCommentPublic[];
  submitted: PullCommentPublic[];
  draft: DraftTarget | null;
  draftBody: string;
  draftBusy: boolean;
  draftError: string | null;
  viewFileHref: string;
  onToggleExpand: () => void;
  onToggleViewed: (viewed: boolean) => void;
  onDraftBody: (body: string) => void;
  onStartDraft: (target: DraftTarget) => void;
  onSubmitDraft: (pending: boolean) => void;
  onCancelDraft: () => void;
  onDiscardPending: (id: string) => void;
}) @{
  const large = props.file.patch.length > DIFF_LARGE_PATCH_CHARS;
  const bodyId = useId();
  const [loadLarge, setLoadLarge] = useState(false);
  const gated = large && !loadLarge;
  const bodyOpen = props.expanded && !gated;
  const { rows, rootRef } = useLazyDiffRows({
    patch: props.file.patch,
    path: props.file.path,
    open: bodyOpen,
    deferPlainRows: large,
  });
  const [adds, dels] = useMemo(() => diffPatchStatCounts(props.file.patch), [props.file.patch]);
  const meta = fileStatusMeta(props.file.status);
  const [dragSel, setDragSel, getDragSel] = useState<DragSelection | null>(null);

  // Old/new line numbers from hunk headers — same gutter math as DiffPatch.
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

  const allComments = useMemo(
    () => [...props.submitted, ...props.pending],
    [props.submitted, props.pending],
  );

  const commentsByRow = useMemo(() => {
    const m = new Map<number, PullCommentPublic[]>();
    for (const c of allComments) {
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
  }, [keyed, allComments]);

  // Rows covered by a committed [start_line..line] range — persistently
  // accent the gutter so the range reads without opening the thread.
  const coveredRows = useMemo(() => {
    const s = new Set<number>();
    if (props.minimizeComments) return s;
    for (const c of allComments) {
      if (c.start_line == null) continue;
      for (const r of keyed) {
        if (commentCoversRow(c, r.kind, r.oldNo, r.newNo)) s.add(r.index);
      }
    }
    return s;
  }, [keyed, allComments, props.minimizeComments]);

  // Rows the open draft covers — GitHub keeps the selected band lit while
  // the composer is open; `finishDrag` clears `dragSel`, so re-derive the
  // range from the draft target (single-line drafts cover just `line`).
  const draftRows = useMemo(() => {
    const s = new Set<number>();
    const d = props.draft;
    if (!d || d.line == null || !d.side) return s;
    const lo = d.startLine ?? d.line;
    for (const r of keyed) {
      const a = anchorForSide(r, d.side);
      if (a && a.line >= lo && a.line <= d.line) s.add(r.index);
    }
    return s;
  }, [keyed, props.draft]);

  const fileComments = useMemo(() => allComments.filter((c) => c.line == null), [allComments]);
  const commentCount = props.pending.length + props.submitted.length;

  // #262 — gutter drag: press on a line-number cell anchors the range and
  // registers the document listeners synchronously (a same-burst mouseup
  // would miss a useEffect-installed listener); hovering rows extends the
  // range, mouseup opens the composer, Escape / window blur cancels.
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
    if (!anchor || !props.canInteract || e.button !== 0) return;
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
    props.onStartDraft({
      path: props.file.path,
      row: Math.max(d.startRow, d.endRow),
      side: d.side,
      line,
      startLine,
    });
  }

  function inDragRange(row: KeyedRow): boolean {
    if (!dragSel) return false;
    const lo = Math.min(dragSel.startRow, dragSel.endRow);
    const hi = Math.max(dragSel.startRow, dragSel.endRow);
    if (row.index < lo || row.index > hi) return false;
    return anchorForSide(row, dragSel.side) != null;
  }

  /** Rows lit as selected — in-flight drag ∪ open-draft range. */
  function inSelRange(row: KeyedRow): boolean {
    return inDragRange(row) || draftRows.has(row.index);
  }

  function startLineDraft(anchor: LineAnchor, rowIndex: number) {
    props.onStartDraft({
      path: props.file.path,
      row: rowIndex,
      side: anchor.side,
      line: anchor.line,
      startLine: null,
    });
  }

  async function copyPath() {
    try {
      await navigator.clipboard.writeText(props.file.path);
    } catch {
      /* clipboard unavailable — ignore */
    }
  }

  const viewed = props.marksEnabled && props.viewed;

  <section
    id={fileAnchorId(props.file.path)}
    ref={rootRef}
    className={
      cn(
        "scroll-mt-32 overflow-hidden rounded-md border border-border bg-card",
        viewed && "opacity-75",
        dragSel && "select-none",
      ) as string
    }
    data-testid="pull-v2-diff-file"
  >
    <div className="flex items-center gap-2 border-b border-border bg-muted/40 px-2 py-1.5">
      <button
        type="button"
        className="inline-flex shrink-0 items-center px-1 text-muted-foreground hover:text-foreground"
        aria-expanded={bodyOpen}
        aria-controls={bodyId}
        aria-label={`Toggle diff for ${props.file.path}`}
        onClick={() => props.onToggleExpand()}
      >
        @if (bodyOpen) {
          <ChevronDown aria-hidden size={14} />;
        } @else {
          <ChevronRight aria-hidden size={14} />;
        }
      </button>
      <span className={cn("shrink-0 font-mono text-[11px]", meta.className as string)}>
        {meta.label as string}
      </span>
      <h3 className="min-w-0 flex-1 truncate font-mono text-[13px] text-foreground">
        {props.file.path as string}
      </h3>
      @if (adds + dels > 0) {
        <span className="shrink-0 tabular-nums text-[12px]">
          <span className="text-success">{`+${adds}` as string}</span>{" "}
          <span className="text-destructive">{`−${dels}` as string}</span>
        </span>;
      }
      @if (commentCount > 0) {
        <span
          className="inline-flex shrink-0 items-center gap-0.5 text-[11px] text-muted-foreground"
          title={`${commentCount} comments`}
          data-testid="pull-v2-file-comment-count"
        >
          <MessageSquare aria-hidden size={10} />
          {commentCount as number}
        </span>;
      }
      @if (props.canInteract) {
        <button
          type="button"
          className="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label={`Comment on ${props.file.path}`}
          title="Comment on this file"
          data-testid="pull-v2-file-comment"
          onClick={() =>
            props.onStartDraft({
              path: props.file.path,
              row: null,
              side: null,
              line: null,
              startLine: null,
            })
          }
        >
          <MessageSquarePlus aria-hidden size={13} />
        </button>;
      }
      @if (props.canInteract && props.marksEnabled) {
        <label className="flex shrink-0 cursor-pointer items-center gap-1.5 text-[12px] text-muted-foreground">
          <span
            className={
              cn(
                "inline-flex size-4 items-center justify-center rounded-sm border",
                props.viewed
                  ? "border-primary bg-primary text-primary-foreground"
                  : "border-input bg-card text-transparent",
              ) as string
            }
            aria-hidden="true"
          >
            <Check size={11} />
          </span>
          Viewed
          <input
            type="checkbox"
            className="sr-only"
            checked={props.viewed}
            data-testid="pull-v2-file-viewed"
            onChange={() => props.onToggleViewed(!props.viewed)}
          />
        </label>;
      }
      <DropdownMenu>
        <DropdownMenuTrigger
          className="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label={`More options for ${props.file.path}`}
          data-testid="pull-v2-file-menu"
        >
          <Ellipsis aria-hidden size={14} />
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-40">
          <DropdownMenuItem data-testid="pull-v2-file-menu-copy" onClick={() => void copyPath()}>
            Copy path
          </DropdownMenuItem>
          <DropdownMenuLinkItem href={props.viewFileHref} data-testid="pull-v2-file-menu-view">
            View file
          </DropdownMenuLinkItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
    @if (props.draft?.row === null || fileComments.length > 0) {
      <div className="border-b border-border/50" data-testid="pull-v2-file-comments">
        <RowNotes
          owner={props.owner}
          repoName={props.repoName}
          draft={props.draft?.row === null ? props.draft : null}
          draftBody={props.draftBody}
          draftBusy={props.draftBusy}
          draftError={props.draftError}
          comments={fileComments}
          canInteract={props.canInteract}
          canWrite={props.canWrite ?? false}
          minimizeComments={props.minimizeComments}
          onDraftBody={props.onDraftBody}
          onSubmitDraft={props.onSubmitDraft}
          onCancelDraft={props.onCancelDraft}
          onDiscardPending={props.onDiscardPending}
        />
      </div>;
    }
    <div id={bodyId}>
      @if (gated) {
        <DiffLargeGate
          onLoad={() => {
            setLoadLarge(true);
          }}
        />;
      } @else {
        @if (props.expanded) {
          @if (keyed.length === 0) {
            @if (props.file.patch.trim().length === 0) {
              <p className="px-3 py-3 text-[13px] text-muted-foreground">
                No textual patch for this file.
              </p>;
            } @else {
              <p className="px-3 py-3 text-[13px] text-muted-foreground">Rendering diff…</p>;
            }
          } @else {
            @if (props.view === "unified") {
              <div
                className={
                  cn(
                    "diff-code overflow-x-auto font-mono leading-[1.45]",
                    props.wrap ? "whitespace-pre-wrap" : "whitespace-pre",
                    props.compact ? "text-[12px]" : "text-[13px]",
                  ) as string
                }
                role="table"
                aria-label={`Diff for ${props.file.path}`}
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
                          "flex",
                          diffLineClass(line.kind) as string,
                          props.compact ? "py-0" : "py-0.5",
                          inSelRange(line) &&
                            "outline-1 -outline-offset-1 outline-primary/60",
                          coveredRows.has(line.index) &&
                            "shadow-[inset_2px_0_0_0_var(--primary)]",
                        ) as string
                      }
                      data-diff-kind={line.kind as string}
                      role="row"
                    >
                      <span
                        className="w-6 shrink-0 select-none pl-1"
                        data-diff-gutter
                        data-side={(unifiedAnchor(line)?.side ?? "") as string}
                        onMouseDown={(e) =>
                          beginDrag(e, unifiedAnchor(line), line.index)
                        }
                      >
                        <LineCommentButton
                          anchor={unifiedAnchor(line)}
                          canInteract={props.canInteract}
                          onAdd={(a) => startLineDraft(a, line.index)}
                        />
                      </span>
                      <span
                        aria-hidden="true"
                        className={
                          cn(
                            "w-10 shrink-0 select-none pr-2 text-right",
                            inSelRange(line) || coveredRows.has(line.index)
                              ? "bg-primary/10 text-primary"
                              : "text-muted-foreground/60",
                          ) as string
                        }
                        data-diff-gutter
                        onMouseDown={(e) =>
                          beginDrag(e, unifiedAnchor(line), line.index)
                        }
                      >
                        {(line.oldNo ?? "") as number | string}
                      </span>
                      <span
                        aria-hidden="true"
                        className={
                          cn(
                            "w-10 shrink-0 select-none border-r border-border/60 pr-2 text-right",
                            inSelRange(line) || coveredRows.has(line.index)
                              ? "bg-primary/10 text-primary"
                              : "text-muted-foreground/60",
                          ) as string
                        }
                        data-diff-gutter
                        onMouseDown={(e) =>
                          beginDrag(e, unifiedAnchor(line), line.index)
                        }
                      >
                        {(line.newNo ?? "") as number | string}
                      </span>
                      @if (line.prefix) {
                        <span
                          className={
                            cn(
                              "shrink-0 select-none pl-3",
                              prefixClass(line.kind),
                            ) as string
                          }
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
                    <RowNotes
                      owner={props.owner}
                      repoName={props.repoName}
                      draft={props.draft?.row === line.index ? props.draft : null}
                      draftBody={props.draftBody}
                      draftBusy={props.draftBusy}
                      draftError={props.draftError}
                      comments={commentsByRow.get(line.index) ?? []}
                      canInteract={props.canInteract}
                      canWrite={props.canWrite ?? false}
                      minimizeComments={props.minimizeComments}
                      onDraftBody={props.onDraftBody}
                      onSubmitDraft={props.onSubmitDraft}
                      onCancelDraft={props.onCancelDraft}
                      onDiscardPending={props.onDiscardPending}
                    />
                  </div>;
                }
              </div>;
            } @else {
              <div
                className={
                  cn(
                    "diff-code overflow-x-auto font-mono leading-[1.45]",
                    props.wrap ? "whitespace-pre-wrap" : "whitespace-pre",
                    props.compact ? "text-[12px]" : "text-[13px]",
                  ) as string
                }
                role="table"
                aria-label={`Diff for ${props.file.path}`}
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
                          "grid grid-cols-2 border-b border-border/40",
                          diffLineClass(line.kind) as string,
                          inSelRange(line) &&
                            "outline-1 -outline-offset-1 outline-primary/60",
                          coveredRows.has(line.index) &&
                            "shadow-[inset_2px_0_0_0_var(--primary)]",
                        ) as string
                      }
                      data-diff-kind={line.kind as string}
                      role="row"
                    >
                      <div className="flex border-r border-border/50 text-muted-foreground">
                        <span
                          className="w-6 shrink-0 select-none pl-1"
                          data-diff-gutter
                          data-side="LEFT"
                          onMouseDown={(e) =>
                            beginDrag(e, splitAnchors(line).left, line.index)
                          }
                        >
                          <LineCommentButton
                            anchor={splitAnchors(line).left}
                            canInteract={props.canInteract}
                            onAdd={(a) => startLineDraft(a, line.index)}
                          />
                        </span>
                        <span
                          aria-hidden="true"
                          className={
                            cn(
                              "w-10 shrink-0 select-none pr-2 text-right",
                              inSelRange(line) || coveredRows.has(line.index)
                                ? "bg-primary/10 text-primary"
                                : "text-muted-foreground/60",
                            ) as string
                          }
                          data-diff-gutter
                          data-side="LEFT"
                          onMouseDown={(e) =>
                            beginDrag(e, splitAnchors(line).left, line.index)
                          }
                        >
                          {(line.oldNo ?? "") as number | string}
                        </span>
                        @if (line.kind !== "add") {
                          <span
                            className="min-w-0 grow px-2 py-0.5 text-foreground"
                            dangerouslySetInnerHTML={{
                              __html: line.contentHtml as string,
                            }}
                          />;
                        } @else {
                          <span className="min-w-0 grow" />;
                        }
                      </div>
                      <div className="flex text-muted-foreground">
                        <span
                          className="w-6 shrink-0 select-none pl-1"
                          data-diff-gutter
                          data-side="RIGHT"
                          onMouseDown={(e) =>
                            beginDrag(e, splitAnchors(line).right, line.index)
                          }
                        >
                          <LineCommentButton
                            anchor={splitAnchors(line).right}
                            canInteract={props.canInteract}
                            onAdd={(a) => startLineDraft(a, line.index)}
                          />
                        </span>
                        <span
                          aria-hidden="true"
                          className={
                            cn(
                              "w-10 shrink-0 select-none pr-2 text-right",
                              inSelRange(line) || coveredRows.has(line.index)
                                ? "bg-primary/10 text-primary"
                                : "text-muted-foreground/60",
                            ) as string
                          }
                          data-diff-gutter
                          data-side="RIGHT"
                          onMouseDown={(e) =>
                            beginDrag(e, splitAnchors(line).right, line.index)
                          }
                        >
                          {(line.newNo ?? "") as number | string}
                        </span>
                        @if (line.kind !== "del") {
                          <span
                            className="min-w-0 grow px-2 py-0.5 text-foreground"
                            dangerouslySetInnerHTML={{
                              __html: line.contentHtml as string,
                            }}
                          />;
                        } @else {
                          <span className="min-w-0 grow" />;
                        }
                      </div>
                    </div>
                    <RowNotes
                      owner={props.owner}
                      repoName={props.repoName}
                      draft={props.draft?.row === line.index ? props.draft : null}
                      draftBody={props.draftBody}
                      draftBusy={props.draftBusy}
                      draftError={props.draftError}
                      comments={commentsByRow.get(line.index) ?? []}
                      canInteract={props.canInteract}
                      canWrite={props.canWrite ?? false}
                      minimizeComments={props.minimizeComments}
                      onDraftBody={props.onDraftBody}
                      onSubmitDraft={props.onSubmitDraft}
                      onCancelDraft={props.onCancelDraft}
                      onDiscardPending={props.onDiscardPending}
                    />
                  </div>;
                }
              </div>;
            }
          }
        }
      }
    </div>
  </section>;
}
