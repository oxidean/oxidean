import { useQuery } from "@octanejs/tanstack-query";
import type { CommitStatusPublic } from "@oxidean/api-client";
import { CheckCircle2, Circle, ListChecks, Loader2, XCircle } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";

export type PullChecksProps = {
  owner: string;
  repoName: string;
  /** Head SHA whose commit statuses gate the merge (PR head commit). */
  sha: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** Map a commit-status state to a status Badge variant. */
function checkStateVariant(state: string): "success" | "destructive" | "primary" | "default" {
  if (state === "success") return "success";
  if (state === "failure" || state === "error") return "destructive";
  if (state === "pending") return "primary";
  return "default";
}

/** Leading status glyph for a check row. */
function CheckStateIcon({ state }: { state: string }) {
  if (state === "success") {
    return <CheckCircle2 aria-hidden size={18} className="shrink-0 text-green-600" />;
  }
  if (state === "failure" || state === "error") {
    return <XCircle aria-hidden size={18} className="shrink-0 text-destructive" />;
  }
  if (state === "pending") {
    return <Loader2 aria-hidden size={18} className="shrink-0 animate-spin text-amber-500" />;
  }
  return <Circle aria-hidden size={18} className="shrink-0 text-muted-foreground" />;
}

function isAuthError(error: unknown): boolean {
  return error instanceof Error && error.message.startsWith("auth.");
}

/**
 * Checks tab (DEBT-05): commit statuses reported for the PR head SHA via
 * `repo.commitStatus.list`. One row per context — state icon, name, description,
 * target link, and last-updated time.
 */
export function PullChecks(props: PullChecksProps) @{
  const checksQ = useQuery({
    queryKey: ["repo", "commitStatus", "list", props.owner, props.repoName, props.sha] as const,
    queryFn: async () => {
      const res = await apiClient.repo.commitStatus.list({
        owner: props.owner,
        name: props.repoName,
        sha: props.sha,
      });
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message || NETWORK_ERROR}`);
      }
      return res.data;
    },
    enabled: !!props.sha,
    refetchInterval: 10_000,
  });

  const statuses: CommitStatusPublic[] = checksQ.data?.statuses ?? [];
  const total = statuses.length;
  const okCount = statuses.filter((s) => s.state === "success").length;
  const pendingCount = statuses.filter((s) => s.state === "pending").length;
  const failingCount = total - okCount - pendingCount;
  const parts: string[] = [];
  if (okCount > 0) parts.push(`${okCount} successful`);
  if (pendingCount > 0) parts.push(`${pendingCount} pending`);
  if (failingCount > 0) parts.push(`${failingCount} failing`);
  const summary =
    `${total} check${total === 1 ? "" : "s"} on ${props.sha.slice(0, 7)}` +
    (parts.length > 0 ? ` · ${parts.join(", ")}` : "");

  <div className="space-y-3" data-testid="pull-checks">
    @if (checksQ.isPending) {
      <List>
        @for (const i of [0, 1, 2]; key i) {
          <ListRow className="gap-3">
            <Skeleton className="h-5 w-5 rounded-full" />
            <Skeleton className="h-4 flex-1" />
            <Skeleton className="h-5 w-16 rounded-md" />
          </ListRow>;
        }
      </List>;
    } @else {
      @if (checksQ.isError) {
        @if (isAuthError(checksQ.error)) {
          <p className="text-sm text-muted-foreground" data-testid="pull-checks-signin">
            Sign in to view checks for this pull request.
          </p>;
        } @else {
          <p role="alert" className="text-sm text-destructive">
            Could not load checks.
          </p>;
        }
      } @else {
        @if (statuses.length === 0) {
          <EmptyState
            compact
            icon={<ListChecks aria-hidden size={20} />}
            title="No checks"
            description="No status checks have been reported for this commit yet."
          />;
        } @else {
          <>
            <p
              className="text-[13px] text-muted-foreground"
              data-testid="pull-checks-summary"
            >
              {summary as string}
            </p>
            <List>
              @for (const s of statuses; key s.context) {
                <ListRow className="gap-3" data-testid="pull-check-row">
                  <CheckStateIcon state={s.state} />
                  <div className="min-w-0 flex-1">
                    @if (s.target_url) {
                      <a
                        href={(s.target_url ?? "#") as string}
                        target="_blank"
                        rel="noreferrer"
                        className="font-medium text-foreground no-underline hover:text-primary hover:underline"
                      >
                        {s.context as string}
                      </a>;
                    } @else {
                      <span className="font-medium text-foreground">
                        {s.context as string}
                      </span>;
                    }
                    @if (s.description) {
                      <p className="mt-0.5 truncate text-[12px] text-muted-foreground">
                        {s.description as string}
                      </p>;
                    }
                  </div>
                  <div className="flex shrink-0 flex-col items-end gap-1">
                    <Badge variant={checkStateVariant(s.state)}>
                      {s.state as string}
                    </Badge>
                    <span className="text-[12px] text-muted-foreground">
                      {formatRelative(s.updated_at) as string}
                    </span>
                  </div>
                </ListRow>;
              }
            </List>
          </>;
        }
      }
    }
  </div>;
}
