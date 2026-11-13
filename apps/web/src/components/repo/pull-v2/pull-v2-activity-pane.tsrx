import { useQuery } from "@octanejs/tanstack-query";
import type { PullCommentPublic, PullCommitSummary, PullReviewPublic } from "@oxidean/api-client";
import { GitCommitHorizontal, GitPullRequest, MessageSquare } from "@octanejs/lucide";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { fileAnchorId } from "./pull-v2-diff-utils";

type PullV2ActivityPaneProps = {
  owner: string;
  repoName: string;
  number: number;
  onJump: (path: string) => void;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type ActivityItem = {
  id: string;
  at: string;
  kind: "commit" | "comment" | "review";
  commit?: PullCommitSummary;
  comment?: PullCommentPublic;
  review?: PullReviewPublic;
};

function reviewVerb(state: string): string {
  if (state === "approved") return "approved these changes";
  if (state === "changes_requested") return "requested changes";
  if (state === "dismissed") return "dismissed a review";
  return "reviewed";
}

function ActivityRow(props: { item: ActivityItem; onJump: (path: string) => void }) @{
  const it = props.item;

  <li className="flex gap-2 py-2 text-[13px]" data-testid="pull-v2-activity-item">
    <span className="mt-0.5 shrink-0 text-muted-foreground" aria-hidden>
      @if (it.kind === "commit") {
        <GitCommitHorizontal size={14} />;
      }
      @if (it.kind === "comment") {
        <MessageSquare size={14} />;
      }
      @if (it.kind === "review") {
        <GitPullRequest size={14} />;
      }
    </span>
    <div className="min-w-0 flex-1">
      @if (it.kind === "commit" && it.commit) {
        <>
          <p className="truncate text-foreground">{it.commit.subject as string}</p>
          <p className="text-[12px] text-muted-foreground" title={it.at as string}>
            {
              `${it.commit.author_username ?? it.commit.author_name} committed ${formatRelative(it.at)}` as string
            }
          </p>
        </>;
      }
      @if (it.kind === "comment" && it.comment) {
        <>
          <p className="text-foreground">
            <span className="font-medium">{it.comment.author_username as string}</span>{" "}
            commented
            @if (it.comment.path) {
              <>
                {" on "}
                {/* raw-anchor-ok: in-page #fileAnchorId jump */}
                <a
                  href={`#${fileAnchorId(it.comment.path ?? "")}` as string}
                  className="font-mono text-[12px] text-primary no-underline hover:underline"
                  onClick={(e) => {
                    e.preventDefault();
                    if (it.comment?.path) props.onJump(it.comment.path);
                  }}
                >
                  {
                    `${it.comment.path}${it.comment.line != null ? `:${it.comment.line}` : ""}` as string
                  }
                </a>
              </>;
            }
          </p>
          <p className="text-[12px] text-muted-foreground" title={it.at as string}>
            {formatRelative(it.at) as string}
          </p>
        </>;
      }
      @if (it.kind === "review" && it.review) {
        <>
          <p className="text-foreground">
            <span className="font-medium">{it.review.author_username as string}</span>
            {` ${reviewVerb(it.review.state)}` as string}
          </p>
          <p className="text-[12px] text-muted-foreground" title={it.at as string}>
            {formatRelative(it.at) as string}
          </p>
        </>;
      }
    </div>
  </li>;
}

/**
 * "Activity" right pane for the v2 pull view (GitHub new-UI pane selector):
 * a reverse-chronological feed of commits, posted comments, and submitted
 * reviews from the same query keys the rest of the page already uses.
 */
export function PullV2ActivityPane(props: PullV2ActivityPaneProps) @{
  const commitsQ = useQuery({
    queryKey: ["pull", "commits", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.commits({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });
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
  const reviewsQ = useQuery({
    queryKey: ["pull", "reviews", props.owner, props.repoName, props.number] as const,
    queryFn: async () => {
      const res = await apiClient.pull.reviews.list({
        owner: props.owner,
        name: props.repoName,
        number: props.number,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });

  const items: ActivityItem[] = [];
  for (const c of commitsQ.data?.commits ?? []) {
    items.push({
      id: `commit-${c.sha}`,
      at: c.authored_at || "",
      kind: "commit",
      commit: c,
    });
  }
  for (const c of commentsQ.data?.comments ?? []) {
    if (c.pending) continue;
    items.push({ id: `comment-${c.id}`, at: c.created_at, kind: "comment", comment: c });
  }
  for (const r of reviewsQ.data?.reviews ?? []) {
    items.push({ id: `review-${r.id}`, at: r.submitted_at, kind: "review", review: r });
  }
  items.sort((a, b) => b.at.localeCompare(a.at));

  <div data-testid="pull-v2-activity-pane">
    @if (
      commitsQ.isPending &&
      commentsQ.isPending &&
      reviewsQ.isPending
    ) {
      <p className="text-[13px] text-muted-foreground">Loading activity…</p>;
    } @else {
      @if (items.length === 0) {
        <p className="text-[13px] text-muted-foreground" data-testid="pull-v2-activity-empty">
          No activity yet.
        </p>;
      } @else {
        <ul className="divide-y divide-border">
          @for (const it of items.slice(0, 50); key it.id) {
            <ActivityRow item={it} onJump={props.onJump} />;
          }
        </ul>;
      }
    }
  </div>;
}
