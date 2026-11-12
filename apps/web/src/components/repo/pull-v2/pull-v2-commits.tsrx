import { useQuery } from "@octanejs/tanstack-query";
import type { PullCommitSummary } from "@oxidean/api-client";
import { GitCommitHorizontal } from "@octanejs/lucide";
import { useMemo } from "octane";
import { CommitAuthorLink } from "@/components/repo/commit-author-link";
import { CommitVerifiedBadge } from "@/components/repo/commit-verified-badge";
import { AppLink } from "@/components/ui/app-link";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { commitHref } from "@/lib/repo-browse";

type PullV2CommitsProps = {
  owner: string;
  repoName: string;
  number: number;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

type CommitGroup = {
  /** `YYYY-MM-DD` sort key, "" for unparseable dates. */
  key: string;
  /** "Commits on Mar 4, 2026" header; "Commits" when the date won't parse. */
  label: string;
  commits: PullCommitSummary[];
};

/**
 * Group commits under GitHub-style "Commits on <date>" headers — keyed on
 * the author date's UTC day (deterministic across timezones).
 */
function groupCommitsByDate(commits: PullCommitSummary[]): CommitGroup[] {
  const groups = new Map<string, CommitGroup>();
  for (const c of commits) {
    const t = Date.parse(c.authored_at);
    let key = "";
    let label = "Commits";
    if (!Number.isNaN(t)) {
      const d = new Date(t);
      key = `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, "0")}-${String(
        d.getUTCDate(),
      ).padStart(2, "0")}`;
      label = `Commits on ${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}`;
    }
    const g = groups.get(key);
    if (g) g.commits.push(c);
    else groups.set(key, { key, label, commits: [c] });
  }
  // Newest day first; "" (unparseable) sinks to the bottom.
  return [...groups.values()].sort((a, b) => (a.key < b.key ? 1 : a.key > b.key ? -1 : 0));
}

/**
 * Commits tab of the new PR experience (WI-0001 S9): commits grouped under
 * per-day headers; the sha links to the sha-scoped changes view
 * (`/pull/N/changes/<sha>`).
 */
export function PullV2Commits(props: PullV2CommitsProps) @{
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

  const commits: PullCommitSummary[] = commitsQ.data?.commits ?? [];
  const groups = useMemo(() => groupCommitsByDate(commits), [commits]);
  const base = `/${props.owner}/${props.repoName}/pull/${props.number}`;

  <div data-testid="pull-v2-commits">
    @if (commitsQ.isPending) {
      <List>
        @for (const i of [0, 1, 2]; key i) {
          <ListRow className="gap-3">
            <Skeleton className="h-4 w-14" />
            <Skeleton className="h-4 flex-1" />
          </ListRow>;
        }
      </List>;
    } @else {
      @if (commitsQ.isError) {
        <p role="alert" className="text-sm text-destructive">
          Could not load commits.
        </p>;
      } @else {
        @if (commits.length === 0) {
          <EmptyState
            compact
            icon={<GitCommitHorizontal aria-hidden size={20} />}
            title="No commits"
            description="This pull request doesn't contain any commits yet."
          />;
        } @else {
          <div className="space-y-4">
            @for (const g of groups; key g.key || "undated") {
              <section data-testid="pull-v2-commit-group">
                <h3 className="mb-1.5 flex items-center gap-1.5 text-[13px] font-semibold text-foreground">
                  <GitCommitHorizontal
                    aria-hidden
                    size={14}
                    className="text-muted-foreground"
                  />
                  {g.label as string}
                </h3>
                <List>
                  @for (const c of g.commits; key c.sha) {
                    <ListRow className="flex-wrap items-baseline gap-3 text-sm">
                      <AppLink
                        href={`${base}/changes/${c.sha}` as string}
                        className="font-mono text-[13px] text-primary no-underline hover:underline"
                        title="View changes in this commit"
                      >
                        {c.short_sha as string}
                      </AppLink>
                      <AppLink
                        href={
                          commitHref(props.owner, props.repoName, c.sha) as string
                        }
                        className="min-w-0 flex-1 truncate font-medium text-foreground no-underline hover:text-primary"
                      >
                        {c.subject as string}
                      </AppLink>
                      <CommitVerifiedBadge
                        status={c.signature_status}
                        kind={c.signature_kind}
                      />
                      <CommitAuthorLink
                        name={c.author_name}
                        username={c.author_username}
                        avatarUrl={c.author_avatar_url}
                        className="text-[12px]"
                      />
                      <span className="text-[12px] text-muted-foreground">
                        {c.authored_at as string}
                      </span>
                    </ListRow>;
                  }
                </List>
              </section>;
            }
          </div>;
        }
      }
    }
  </div>;
}
