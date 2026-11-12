import { useQueryClient } from "@octanejs/tanstack-query";
import type { PullCommentPublic } from "@oxidean/api-client";
import { Trash2, X } from "@octanejs/lucide";
import { useState } from "octane";
import { Badge } from "@/components/ui/badge";
import { apiClient } from "@/lib/api-client";
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

function PanelCommentRow(props: {
  comment: PullCommentPublic;
  pending: boolean;
  canDiscard: boolean;
  discarding: boolean;
  onDiscard: (c: PullCommentPublic) => void;
  onJump: (path: string) => void;
}) @{
  const c = props.comment;

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
      @if (props.pending) {
        <Badge variant="primary">Pending</Badge>;
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
      @if (props.pending && props.canDiscard) {
        <button
          type="button"
          className="inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-destructive"
          aria-label="Discard pending comment"
          data-testid="pull-v2-panel-discard"
          disabled={props.discarding}
          onClick={() => props.onDiscard(c)}
        >
          <Trash2 aria-hidden size={12} />
        </button>;
      }
    </div>
    <p className="mt-0.5 line-clamp-3 text-muted-foreground">{excerpt(c.body) as string}</p>
    <p className="mt-0.5 text-[11px] text-muted-foreground">
      {`@${c.author_username} · ${formatRelative(c.created_at)}` as string}
    </p>
  </li>;
}

/**
 * Right-side comments index for the v2 pull experience (WI-0001 S7):
 * every pull review comment with All / Unresolved / Mine filter chips and
 * jump-to-file links. Server-persisted review drafts (pending === true)
 * group at the top with a discard affordance; local (client-side) drafts
 * still report through `pendingCount` as "not shown".
 */
export function PullV2CommentsPanel(props: {
  comments: PullCommentPublic[];
  /** Local drafts not yet posted server-side (changes tab only). */
  pendingCount?: number;
  currentUserId: string;
  /** Repo coordinates enable the pending-draft discard RPC. */
  owner?: string;
  repoName?: string;
  number?: number;
  canInteract?: boolean;
  onClose: () => void;
  onJump: (path: string) => void;
}) @{
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState<CommentFilter>("all");
  const [discarding, setDiscarding] = useState<string | null>(null);
  const [error, setError] = useState("");

  const filtered = props.comments.filter((c) => {
    if (filter === "unresolved") return !!c.path && !c.resolved;
    if (filter === "mine") return c.author_id === props.currentUserId;
    return true;
  });
  const pendingItems = filtered.filter((c) => c.pending);
  const postedItems = filtered.filter((c) => !c.pending);
  const localDrafts = props.pendingCount ?? 0;
  const canDiscard = !!(props.canInteract && props.owner && props.repoName && props.number);

  async function discard(c: PullCommentPublic) {
    if (!canDiscard) return;
    setDiscarding(c.id);
    setError("");
    try {
      const res = await apiClient.pull.comments.delete({
        owner: props.owner ?? "",
        name: props.repoName ?? "",
        number: props.number ?? 0,
        commentId: c.id,
      });
      setDiscarding(null);
      if (!res.ok) {
        setError(res.error.message || "Could not discard the pending comment");
        return;
      }
      await queryClient.invalidateQueries({
        queryKey: ["pull", "comments", props.owner, props.repoName, props.number],
      });
    } catch {
      setDiscarding(null);
      setError("Can't reach Oxidean. Check your connection and try again.");
    }
  }

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
    <div className="max-h-[60vh] overflow-y-auto">
      @if (pendingItems.length > 0) {
        <>
          <p
            className="border-b border-border bg-muted/40 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground"
            data-testid="pull-v2-pending-group"
          >
            {`Pending (${pendingItems.length})` as string}
          </p>
          <ul className="divide-y divide-border">
            @for (const c of pendingItems; key c.id) {
              <PanelCommentRow
                comment={c}
                pending
                canDiscard={canDiscard}
                discarding={discarding === c.id}
                onDiscard={(x) => void discard(x)}
                onJump={props.onJump}
              />;
            }
          </ul>
        </>;
      }
      <ul className="divide-y divide-border">
        @for (const c of postedItems; key c.id) {
          <PanelCommentRow
            comment={c}
            pending={false}
            canDiscard={false}
            discarding={false}
            onDiscard={() => {}}
            onJump={props.onJump}
          />;
        }
        @if (filtered.length === 0) {
          <li className="px-3 py-6 text-center text-[13px] text-muted-foreground">
            No comments match this filter.
          </li>;
        }
      </ul>
    </div>
    @if (error) {
      <p
        className="border-t border-border px-3 py-2 text-[12px] text-destructive"
        role="alert"
        data-testid="pull-v2-panel-error"
      >
        {error as string}
      </p>;
    }
    @if (localDrafts > 0) {
      <p className="border-t border-border px-3 py-2 text-[12px] text-muted-foreground">
        {
          `${localDrafts} pending comment${localDrafts === 1 ? "" : "s"} not shown — submit the review to post them.` as string
        }
      </p>;
    }
  </div>;
}
