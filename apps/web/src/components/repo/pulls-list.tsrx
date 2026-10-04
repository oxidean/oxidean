import { GitMerge, GitPullRequest, GitPullRequestClosed, MessageSquare } from "@octanejs/lucide";
import type { IssueAssigneePublic, PullListResponse, PullPublic } from "@oxidean/api-client";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { formatRelative } from "@/lib/relative-time";
import { AppLink } from "@/components/ui/app-link";

type PullsListProps = {
  owner: string;
  repo: string;
  list: PullListResponse;
};

function assigneeLine(assignees: IssueAssigneePublic[] | undefined): string | null {
  if (!assignees || assignees.length === 0) return null;
  return assignees.map((a) => `@${a.username}`).join(", ");
}

function statusLine(pull: PullPublic): string {
  if (pull.state === "merged") {
    return `#${pull.number} by @${pull.author_username} was merged ${formatRelative(pull.merged_at ?? pull.updated_at)}`;
  }
  if (pull.state === "closed") {
    return `#${pull.number} by @${pull.author_username} was closed ${formatRelative(pull.closed_at ?? pull.updated_at)}`;
  }
  return `#${pull.number} opened ${formatRelative(pull.created_at)} by @${pull.author_username}`;
}

function headLabel(pull: PullPublic, owner: string, repo: string): string {
  const sameRepo = pull.head_owner === owner && pull.head_name === repo;
  return sameRepo ? pull.head_ref : `${pull.head_owner}:${pull.head_ref}`;
}

export function PullsList(props: PullsListProps) @{
  const { owner, repo, list } = props;

  @if (list.pulls.length === 0) {
    <EmptyState
      icon={<GitPullRequest aria-hidden size={22} />}
      title="No pull requests"
      description="No pull requests match this filter. Open one to propose changes."
      action={
        <AppLink href={`/${owner}/${repo}/pulls/new` as string} className="no-underline">
          <Button type="button" size="sm">
            New pull request
          </Button>
        </AppLink>
      }
    />;
  } @else {
    <List>
      @for (const pull of list.pulls; key pull.id) {
        <ListRow className="items-start gap-3 py-3">
          @if (pull.state === "merged") {
            <GitMerge
              aria-hidden
              size={16}
              className="mt-0.5 shrink-0 text-[#8250df] dark:text-[#a371f7]"
            />;
          } @else if (pull.state === "closed") {
            <GitPullRequestClosed
              aria-hidden
              size={16}
              className="mt-0.5 shrink-0 text-destructive"
            />;
          } @else {
            <GitPullRequest
              aria-hidden
              size={16}
              className="mt-0.5 shrink-0 text-success"
            />;
          }
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
              <AppLink
                href={`/${owner}/${repo}/pull/${pull.number}` as string}
                className="text-[15px] font-semibold text-foreground no-underline hover:text-primary"
              >
                {pull.title as string}
              </AppLink>
              @if (pull.draft) {
                <Badge variant="draft">Draft</Badge>;
              }
            </div>
            <p className="mt-1 text-[12px] text-muted-foreground">
              {statusLine(pull) as string}
              <span className="mx-1.5">·</span>
              <span className="font-mono">{headLabel(pull, owner, repo) as string}</span>
              <span className="mx-1">→</span>
              <span className="font-mono">{pull.base_ref as string}</span>
              @if (assigneeLine(pull.assignees)) {
                <>
                  <span className="mx-1.5">·</span>
                  <span>{`assigned ${assigneeLine(pull.assignees)}` as string}</span>
                </>;
              }
            </p>
          </div>
          @if ((pull.comment_count ?? 0) > 0) {
            <AppLink
              href={`/${owner}/${repo}/pull/${pull.number}` as string}
              className="mt-0.5 flex shrink-0 items-center gap-1 text-[12px] text-muted-foreground no-underline hover:text-primary"
              aria-label={
                `${pull.comment_count} comment${pull.comment_count === 1 ? "" : "s"}` as string
              }
            >
              <MessageSquare aria-hidden size={14} />
              {pull.comment_count as number}
            </AppLink>;
          }
        </ListRow>;
      }
    </List>;
  }
}
