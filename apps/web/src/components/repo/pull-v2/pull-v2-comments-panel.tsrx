import type { PullCommentPublic } from "@oxidean/api-client";
import { X } from "@octanejs/lucide";
import { useState } from "octane";
import { formatRelative } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import { fileAnchorId } from "./pull-v2-diff-utils";

type CommentFilter = "all" | "unresolved" | "mine";

const FILTERS: { id: CommentFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "unresolved", label: "Unresolved" },
  { id: "mine", label: "Mine" },
];

function excerpt(body: string): string {
  const flat = body.replace(/\s+/g, " ").trim();
  return flat.length > 160 ? `${flat.slice(0, 160)}…` : flat;
}

/**
 * Right-side comments index for the v2 changes tab (WI-0001 S7): every
 * pull review comment with All / Unresolved / Mine filter chips and
 * jump-to-file links.
 */
export function PullV2CommentsPanel(props: {
  comments: PullCommentPublic[];
  pendingCount: number;
  currentUserId: string;
  onClose: () => void;
  onJump: (path: string) => void;
}) @{
  const [filter, setFilter] = useState<CommentFilter>("all");

  const filtered = props.comments.filter((c) => {
    if (filter === "unresolved") return !c.resolved;
    if (filter === "mine") return c.author_id === props.currentUserId;
    return true;
  });

  <div className="flex flex-col" data-testid="pull-v2-comments-panel">
    <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
      <h3 className="text-[13px] font-semibold text-foreground">
        {`Comments (${props.comments.length})` as string}
      </h3>
      <button
        type="button"
        className="inline-flex size-6 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
        aria-label="Close comments panel"
        data-testid="pull-v2-comments-close"
        onClick={() => props.onClose()}
      >
        <X aria-hidden size={14} />
      </button>
    </div>
    <div
      className="flex gap-1 border-b border-border px-3 py-2"
      role="group"
      aria-label="Filter comments"
    >
      @for (const f of FILTERS; key f.id) {
        <button
          type="button"
          className={
            cn(
              "rounded-full px-2 py-0.5 text-[12px]",
              filter === f.id
                ? "bg-primary/10 font-semibold text-primary"
                : "text-muted-foreground hover:bg-muted",
            ) as string
          }
          data-testid={`pull-v2-comments-filter-${f.id}`}
          onClick={() => setFilter(f.id)}
        >
          {f.label as string}
        </button>;
      }
    </div>
    <ul className="max-h-[60vh] divide-y divide-border overflow-y-auto">
      @for (const c of filtered; key c.id) {
        <li className="px-3 py-2 text-[13px]" data-testid="pull-v2-panel-comment">
          <div className="flex items-baseline gap-1.5">
            @if (c.path) {
              {
                /* raw-anchor-ok — same-page hash jump to the diff block */
              }
              <a
                href={`#${fileAnchorId(c.path ?? "")}` as string}
                className="min-w-0 flex-1 truncate font-mono text-[12px] text-primary no-underline hover:underline"
                title={c.path as string}
                onClick={(e) => {
                  e.preventDefault();
                  if (c.path) props.onJump(c.path);
                }}
              >
                {`${c.path}:${c.line ?? "?"}` as string}
              </a>;
            } @else {
              <span className="min-w-0 flex-1 truncate text-[12px] text-muted-foreground">
                General comment
              </span>;
            }
            @if (c.resolved) {
              <span className="shrink-0 text-[11px] uppercase tracking-wide text-muted-foreground">
                Resolved
              </span>;
            }
            @if (c.outdated) {
              <span className="shrink-0 text-[11px] uppercase tracking-wide text-muted-foreground">
                Outdated
              </span>;
            }
          </div>
          <p className="mt-0.5 line-clamp-3 text-muted-foreground">
            {excerpt(c.body) as string}
          </p>
          <p className="mt-0.5 text-[11px] text-muted-foreground">
            {`@${c.author_username} · ${formatRelative(c.created_at)}` as string}
          </p>
        </li>;
      }
      @if (filtered.length === 0) {
        <li className="px-3 py-6 text-center text-[13px] text-muted-foreground">
          No comments match this filter.
        </li>;
      }
    </ul>
    @if (props.pendingCount > 0) {
      <p className="border-t border-border px-3 py-2 text-[12px] text-muted-foreground">
        {
          `${props.pendingCount} pending comment${props.pendingCount === 1 ? "" : "s"} not shown — submit the review to post them.` as string
        }
      </p>;
    }
  </div>;
}
