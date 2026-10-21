import { useQuery } from "@octanejs/tanstack-query";
import type { CheckRunPublic, CommitStatusPublic } from "@oxidean/api-client";
import { CheckCircle2, Circle, ListChecks, Loader2, MinusCircle, XCircle } from "@octanejs/lucide";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";

type PullChecksProps = {
  owner: string;
  repoName: string;
  /** Head SHA whose checks gate the merge (PR head commit). */
  sha: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** Fold a check run's status+conclusion onto the commit-status vocabulary. */
function checkRunState(run: CheckRunPublic): string {
  if (run.status === "completed") return run.conclusion ?? "failure";
  return "pending";
}

/** Map a check state to a status Badge variant. */
function checkStateVariant(state: string): "success" | "destructive" | "primary" | "default" {
  if (state === "success") return "success";
  if (
    state === "failure" ||
    state === "error" ||
    state === "timed_out" ||
    state === "startup_failure"
  )
    return "destructive";
  if (state === "pending" || state === "queued" || state === "in_progress" || state === "waiting")
    return "primary";
  return "default";
}

/** Leading status glyph for a check row. */
function CheckStateIcon({ state }: { state: string }) {
  if (state === "success") {
    return <CheckCircle2 aria-hidden size={18} className="shrink-0 text-green-600" />;
  }
  if (
    state === "failure" ||
    state === "error" ||
    state === "timed_out" ||
    state === "startup_failure"
  ) {
    return <XCircle aria-hidden size={18} className="shrink-0 text-destructive" />;
  }
  if (state === "neutral" || state === "skipped" || state === "stale") {
    return <MinusCircle aria-hidden size={18} className="shrink-0 text-muted-foreground" />;
  }
  if (state === "pending") {
    return <Loader2 aria-hidden size={18} className="shrink-0 animate-spin text-amber-500" />;
  }
  return <Circle aria-hidden size={18} className="shrink-0 text-muted-foreground" />;
}

function isAuthError(error: unknown): boolean {
  return error instanceof Error && error.message.startsWith("auth.");
}

/** `GitHub Actions · 3 annotations · title` — the check run meta line. */
function checkRunMeta(run: CheckRunPublic): string {
  const parts = [run.app === "github-actions" ? "GitHub Actions" : run.app];
  const count = run.output?.annotations_count ?? 0;
  if (count > 0) parts.push(`${count} annotation${count === 1 ? "" : "s"}`);
  if (run.output?.title) parts.push(run.output.title);
  return parts.join(" · ");
}

/**
 * Checks tab (DEBT-05): check runs + commit statuses reported for the PR
 * head SHA. Check runs come from `check.runs.listForRef` (the Checks
 * surface — Actions jobs project here); legacy `repo.commitStatus.list`
 * contexts fill in names no check run covers.
 */
export function PullChecks(props: PullChecksProps) @{
  const runsQ = useQuery({
    queryKey: ["check", "runs", "listForRef", props.owner, props.repoName, props.sha] as const,
    queryFn: async () => {
      const res = await apiClient.check.runs.listForRef({
        owner: props.owner,
        name: props.repoName,
        ref: props.sha,
      });
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message || NETWORK_ERROR}`);
      }
      return res.data;
    },
    enabled: !!props.sha,
    refetchInterval: 10_000,
  });
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

  const checkRuns: CheckRunPublic[] = runsQ.data?.check_runs ?? [];
  const runNames = new Set(checkRuns.map((r) => r.name));
  // Commit statuses fill contexts no check run covers — a same-named check
  // run is the newer writer (GitHub parity).
  const statuses: CommitStatusPublic[] = (checksQ.data?.statuses ?? []).filter(
    (s) => !runNames.has(s.context),
  );

  const runStates = checkRuns.map(checkRunState);
  const stateIsOk = (s: string) => s === "success" || s === "neutral" || s === "skipped";
  const stateIsPending = (s: string) => s === "pending";
  const okCount =
    runStates.filter(stateIsOk).length + statuses.filter((s) => s.state === "success").length;
  const pendingCount =
    runStates.filter(stateIsPending).length + statuses.filter((s) => s.state === "pending").length;
  const total = checkRuns.length + statuses.length;
  const failingCount = total - okCount - pendingCount;
  const parts: string[] = [];
  if (okCount > 0) parts.push(`${okCount} successful`);
  if (pendingCount > 0) parts.push(`${pendingCount} pending`);
  if (failingCount > 0) parts.push(`${failingCount} failing`);
  const summary =
    `${total} check${total === 1 ? "" : "s"} on ${props.sha.slice(0, 7)}` +
    (parts.length > 0 ? ` · ${parts.join(", ")}` : "");

  const isPending = runsQ.isPending && checksQ.isPending;
  const queryError = runsQ.error ?? checksQ.error;

  <div className="space-y-3" data-testid="pull-checks">
    @if (isPending) {
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
      @if (queryError) {
        @if (isAuthError(queryError)) {
          <p className="text-sm text-muted-foreground" data-testid="pull-checks-signin">
            Sign in to view checks for this pull request.
          </p>;
        } @else {
          <p role="alert" className="text-sm text-destructive">
            Could not load checks.
          </p>;
        }
      } @else {
        @if (total === 0) {
          <EmptyState
            compact
            icon={<ListChecks aria-hidden size={20} />}
            title="No checks"
            description="No check runs or status checks have been reported for this commit yet."
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
              @for (const r of checkRuns; key r.id) {
                <ListRow className="gap-3" data-testid="pull-check-run-row">
                  <CheckStateIcon state={checkRunState(r)} />
                  <div className="min-w-0 flex-1">
                    @if (r.run_id) {
                      <AppLink
                        href={
                          `/${props.owner}/${props.repoName}/actions/${r.run_id}` as string
                        }
                        className="font-medium text-foreground no-underline hover:text-primary hover:underline"
                      >
                        {r.name as string}
                      </AppLink>;
                    } @else {
                      @if (r.details_url) {
                        {
                          /* raw-anchor-ok — external check details URL */
                        }
                        <a
                          href={(r.details_url ?? "#") as string}
                          target="_blank"
                          rel="noreferrer"
                          className="font-medium text-foreground no-underline hover:text-primary hover:underline"
                        >
                          {r.name as string}
                        </a>;
                      } @else {
                        <span className="font-medium text-foreground">
                          {r.name as string}
                        </span>;
                      }
                    }
                    <p className="mt-0.5 truncate text-[12px] text-muted-foreground">
                      {checkRunMeta(r) as string}
                    </p>
                  </div>
                  <div className="flex shrink-0 flex-col items-end gap-1">
                    <Badge variant={checkStateVariant(checkRunState(r))}>
                      {checkRunState(r) as string}
                    </Badge>
                    <span className="text-[12px] text-muted-foreground">
                      {formatRelative(r.updated_at) as string}
                    </span>
                  </div>
                </ListRow>;
              }
              @for (const s of statuses; key s.context) {
                <ListRow className="gap-3" data-testid="pull-check-row">
                  <CheckStateIcon state={s.state} />
                  <div className="min-w-0 flex-1">
                    @if (s.target_url) {
                      {
                        /* raw-anchor-ok — external CI status URL */
                      }
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
