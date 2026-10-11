import type { RepoCommitSummary } from "@oxidean/api-client";
import { CommitAuthorLink } from "@/components/repo/commit-author-link";
import { CommitVerifiedBadge } from "@/components/repo/commit-verified-badge";
import { formatRelativeUpdated } from "@/lib/relative-time";
import { AppLink } from "@/components/ui/app-link";

type RepoLastCommitBarProps = {
  owner: string;
  repo: string;
  refName: string;
  tip: RepoCommitSummary | null;
  commitCount: number | null;
};

function relativeCommitTime(iso: string): string {
  return formatRelativeUpdated(iso).replace(/^Updated\s+/i, "");
}

/** Tip commit strip above the file tree (issue #23). */
export function RepoLastCommitBar({
  owner,
  repo,
  refName,
  tip,
  commitCount,
}: RepoLastCommitBarProps) @{
  const commitsHref = `/${owner}/${repo}/commits/${encodeURIComponent(refName)}`;

  @if (!tip) {
    <div
      className="mb-0 flex min-h-11 items-center rounded-t-md border border-b-0 border-border bg-muted/40 px-3 text-[13px] text-muted-foreground"
      data-testid="repo-last-commit"
    >
      No commits on this branch yet.
    </div>;
  } @else {
    <div
      className="mb-0 flex min-h-11 flex-wrap items-center gap-x-3 gap-y-1 rounded-t-md border border-b-0 border-border bg-muted/40 px-3 py-2 text-[13px]"
      data-testid="repo-last-commit"
    >
      <span className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1 truncate">
        <CommitAuthorLink
          name={tip.author_name}
          username={tip.author_username}
          avatarUrl={tip.author_avatar_url}
        />
        <CommitVerifiedBadge status={tip.signature_status} kind={tip.signature_kind} />
        <span className="text-muted-foreground" aria-hidden="true">
          ·
        </span>
        <AppLink
          href={`/${owner}/${repo}/commit/${tip.sha}` as string}
          className="min-w-0 truncate font-medium text-foreground no-underline hover:text-primary hover:underline"
          title={tip.subject as string}
        >
          {tip.subject as string}
        </AppLink>
      </span>
      <span className="flex shrink-0 flex-wrap items-center gap-3 text-muted-foreground">
        <AppLink
          href={`/${owner}/${repo}/commit/${tip.sha}` as string}
          className="font-mono text-primary no-underline hover:underline"
        >
          {tip.short_sha as string}
        </AppLink>
        <time datetime={tip.authored_at as string}>
          {relativeCommitTime(tip.authored_at) as string}
        </time>
        @if (commitCount != null && commitCount > 0) {
          <AppLink
            href={commitsHref as string}
            className="no-underline hover:text-foreground hover:underline"
          >
            {(String(commitCount) + (commitCount === 1 ? " commit" : " commits")) as string}
          </AppLink>;
        }
      </span>
    </div>;
  }
}
