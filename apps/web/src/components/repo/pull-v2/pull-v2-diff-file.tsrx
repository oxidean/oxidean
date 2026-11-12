import type { PullCommentPublic, PullDiffFile } from "@oxidean/api-client";
import { Check, ChevronDown, ChevronRight, MessageSquarePlus } from "@octanejs/lucide";
import { useId, useMemo, useState } from "octane";
import { DiffLargeGate } from "@/components/repo/diff-file-header";
import { Textarea } from "@/components/ui/textarea";
import { DIFF_LARGE_PATCH_CHARS } from "@/lib/diff-budget";
import type { HighlightedDiffLine } from "@/lib/highlight-diff";
import { useLazyDiffRows } from "@/lib/lazy-diff-rows.tsrx";
import { diffPatchStatCounts } from "@/lib/parse-unified-diff";
import { formatRelative } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import {
  diffLineClass,
  fileAnchorId,
  fileStatusMeta,
  type DraftTarget,
  type PendingLineComment,
} from "./pull-v2-diff-utils";

type KeyedRow = HighlightedDiffLine & {
  index: number;
  oldNo: number | null;
  newNo: number | null;
};

type LineAnchor = { side: "LEFT" | "RIGHT"; line: number };

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

function prefixClass(kind: string): string {
  if (kind === "add") return "text-[var(--diff-add-fg)]";
  if (kind === "del") return "text-[var(--diff-del-fg)]";
  return "text-muted-foreground";
}

/**
 * "+ comment" gutter button (WI-0001 S6). Lines that cannot be anchored
 * (hunk/meta/note rows carry no line number) render a disabled button.
 */
function LineCommentButton(props: {
  anchor: LineAnchor | null;
  canInteract: boolean;
  onAdd: (anchor: LineAnchor) => void;
}) @{
  <span className="w-6 shrink-0 select-none pl-1">
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
  </span>;
}

/** Inline draft box under the row being commented on. */
function LineDraftBox(props: {
  body: string;
  onBody: (next: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
}) @{
  <div className="border-t border-border/50 bg-muted/20 px-8 py-2" data-testid="pull-v2-line-draft">
    <Textarea
      rows={3}
      className="font-sans text-[13px]"
      placeholder="Leave a review comment"
      value={props.body}
      onInput={(e) => props.onBody((e.currentTarget as HTMLTextAreaElement).value)}
    />
    <div className="mt-2 flex gap-2">
      <button
        type="button"
        className="rounded-md bg-primary px-2 py-1 text-[13px] font-medium text-primary-foreground disabled:opacity-60"
        disabled={!props.body.trim()}
        data-testid="pull-v2-line-draft-add"
        onClick={() => props.onSubmit()}
      >
        Add review comment
      </button>
      <button
        type="button"
        className="rounded-md px-2 py-1 text-[13px] text-muted-foreground hover:bg-muted"
        onClick={() => props.onCancel()}
      >
        Cancel
      </button>
    </div>
  </div>;
}

/** Locally-pending review comment shown under its line (S6). */
function PendingNote(props: {
  comment: PendingLineComment;
  onDiscard: (id: number) => void;
}) @{
  <div
    className="border-t border-primary/20 bg-primary/5 px-8 py-1.5 text-[13px]"
    data-testid="pull-v2-pending-comment"
  >
    <div className="flex items-center gap-2">
      <span className="rounded bg-primary/15 px-1.5 py-0.5 text-[11px] font-semibold uppercase tracking-wide text-primary">
        Pending
      </span>
      <span className="font-mono text-[11px] text-muted-foreground">
        {`${props.comment.side === "LEFT" ? "L" : "R"}${props.comment.line}` as string}
      </span>
      <button
        type="button"
        className="ml-auto text-[12px] text-muted-foreground underline-offset-2 hover:underline"
        onClick={() => props.onDiscard(props.comment.id)}
      >
        Discard
      </button>
    </div>
    <p className="mt-1 whitespace-pre-wrap text-foreground">{props.comment.body as string}</p>
  </div>;
}

/** Already-submitted review comment rendered under its line. */
function SubmittedNote(props: { comment: PullCommentPublic }) @{
  <div className="border-t border-border/50 bg-muted/20 px-8 py-1.5 text-[13px]">
    <div className="flex items-center gap-2">
      <span className="font-semibold text-foreground">
        {`@${props.comment.author_username}` as string}
      </span>
      <span className="text-[12px] text-muted-foreground">
        {formatRelative(props.comment.created_at) as string}
      </span>
      @if (props.comment.resolved) {
        <span className="text-[11px] uppercase tracking-wide text-muted-foreground">
          Resolved
        </span>;
      }
      @if (props.comment.outdated) {
        <span className="text-[11px] uppercase tracking-wide text-muted-foreground">
          Outdated
        </span>;
      }
    </div>
    <p className="mt-1 whitespace-pre-wrap text-foreground">{props.comment.body as string}</p>
  </div>;
}

/**
 * Draft box + pending + submitted notes rendered under a diff row.
 * A component (not a JSX-returning helper) — `@if`/`@for` directives are
 * only legal inside `@{ }` bodies.
 */
function RowNotes(props: {
  draft: DraftTarget | null;
  draftBody: string;
  pending: PendingLineComment[];
  submitted: PullCommentPublic[];
  onDraftBody: (body: string) => void;
  onSubmitDraft: () => void;
  onCancelDraft: () => void;
  onDiscardPending: (id: number) => void;
}) @{
  <>
    @if (props.draft) {
      <LineDraftBox
        body={props.draftBody}
        onBody={props.onDraftBody}
        onSubmit={props.onSubmitDraft}
        onCancel={props.onCancelDraft}
      />;
    }
    @for (const pc of props.pending; key pc.id) {
      <PendingNote comment={pc} onDiscard={props.onDiscardPending} />;
    }
    @for (const sc of props.submitted; key sc.id) {
      <SubmittedNote comment={sc} />;
    }
  </>;
}

/**
 * Per-file diff block for the v2 changes tab (WI-0001 S4–S6): same lazy
 * highlight + large-diff gate + collapse behavior as the shared DiffPatch,
 * plus a viewed checkbox in the header and a "+ comment" affordance on any
 * anchorable line feeding the local pending-review list.
 */
export function PullV2DiffFile(props: {
  file: PullDiffFile;
  view: "unified" | "split";
  wrap: boolean;
  compact: boolean;
  expanded: boolean;
  viewed: boolean;
  canInteract: boolean;
  pending: PendingLineComment[];
  submitted: PullCommentPublic[];
  draft: DraftTarget | null;
  draftBody: string;
  onToggleExpand: () => void;
  onToggleViewed: (viewed: boolean) => void;
  onDraftBody: (body: string) => void;
  onStartDraft: (target: DraftTarget) => void;
  onSubmitDraft: () => void;
  onCancelDraft: () => void;
  onDiscardPending: (id: number) => void;
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

  const pendingByRow = useMemo(() => {
    const m = new Map<number, PendingLineComment[]>();
    for (const c of props.pending) {
      const list = m.get(c.row) ?? [];
      list.push(c);
      m.set(c.row, list);
    }
    return m;
  }, [props.pending]);

  const submittedByRow = useMemo(() => {
    const m = new Map<number, PullCommentPublic[]>();
    for (const c of props.submitted) {
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
  }, [keyed, props.submitted]);

  <section
    id={fileAnchorId(props.file.path)}
    ref={rootRef}
    className={
      cn(
        "scroll-mt-32 overflow-hidden rounded-md border border-border bg-card",
        props.viewed && "opacity-75",
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
      @if (props.canInteract) {
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
    </div>
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
                  <div className="group" data-diff-kind={line.kind as string}>
                    <div
                      className={cn(
                        "flex",
                        diffLineClass(line.kind) as string,
                        props.compact ? "py-0" : "py-0.5",
                      )}
                      role="row"
                    >
                      <LineCommentButton
                        anchor={unifiedAnchor(line)}
                        canInteract={props.canInteract}
                        onAdd={(a) =>
                          props.onStartDraft({
                            path: props.file.path,
                            row: line.index,
                            side: a.side,
                            line: a.line,
                          })
                        }
                      />
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
                      draft={props.draft?.row === line.index ? props.draft : null}
                      draftBody={props.draftBody}
                      pending={pendingByRow.get(line.index) ?? []}
                      submitted={submittedByRow.get(line.index) ?? []}
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
                  <div className="group" data-diff-kind={line.kind as string}>
                    <div
                      className={cn(
                        "grid grid-cols-2 border-b border-border/40",
                        diffLineClass(line.kind) as string,
                      )}
                      role="row"
                    >
                      <div className="flex border-r border-border/50 text-muted-foreground">
                        <LineCommentButton
                          anchor={splitAnchors(line).left}
                          canInteract={props.canInteract}
                          onAdd={(a) =>
                            props.onStartDraft({
                              path: props.file.path,
                              row: line.index,
                              side: a.side,
                              line: a.line,
                            })
                          }
                        />
                        <span
                          aria-hidden="true"
                          className="w-10 shrink-0 select-none pr-2 text-right text-muted-foreground/60"
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
                        <LineCommentButton
                          anchor={splitAnchors(line).right}
                          canInteract={props.canInteract}
                          onAdd={(a) =>
                            props.onStartDraft({
                              path: props.file.path,
                              row: line.index,
                              side: a.side,
                              line: a.line,
                            })
                          }
                        />
                        <span
                          aria-hidden="true"
                          className="w-10 shrink-0 select-none pr-2 text-right text-muted-foreground/60"
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
                      draft={props.draft?.row === line.index ? props.draft : null}
                      draftBody={props.draftBody}
                      pending={pendingByRow.get(line.index) ?? []}
                      submitted={submittedByRow.get(line.index) ?? []}
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
