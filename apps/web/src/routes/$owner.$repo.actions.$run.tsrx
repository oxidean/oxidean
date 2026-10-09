import { queryOptions, useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { navigate as astroNavigate } from "astro:transitions/client";

import { matchPath, routeHref, searchRecord } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { ActionJobLogResponse, ActionRunGetResponse } from "@oxidean/api-client";
import { ArrowLeft, Ban, RotateCcw, Workflow } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { ActionsLogViewer } from "@/components/repo/actions-log-viewer";
import { ActionsRerunMenu } from "@/components/repo/actions-rerun-menu";
import { useRepoStore } from "@/lib/repo-store";
import { formatRelative } from "@/lib/relative-time";
import { fetchActionsGetJobLog, fetchActionsGetRun } from "@/lib/ssr-repo";
import { cn } from "@/lib/utils";
import {
  actionsCancelRunMutation,
  actionsJobLogQuery,
  actionsRerunRunMutation,
  actionsRunDetailQuery,
} from "@/lib/actions-queries";
import { AppLink } from "@/components/ui/app-link";

/** Map a workflow run/job status to a status Badge variant. */
function runStatusVariant(status: string): "success" | "destructive" | "primary" | "default" {
  const s = status.toLowerCase();
  if (s === "success" || s === "completed" || s === "passed") return "success";
  if (s === "failure" || s === "failed" || s === "error" || s === "cancelled") return "destructive";
  if (s === "in_progress" || s === "running" || s === "queued" || s === "pending") return "primary";
  return "default";
}

function isActiveStatus(status: string): boolean {
  return ["queued", "in_progress", "pending", "running"].includes(status);
}

/** Short human duration between two ISO timestamps ("1m 5s"). */
function spanDuration(started?: string, finished?: string): string {
  if (!started) return "";
  const start = Date.parse(started);
  const end = finished ? Date.parse(finished) : Date.now();
  if (!Number.isFinite(start) || !Number.isFinite(end) || end < start) return "";
  const secs = Math.floor((end - start) / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ${secs % 60}s`;
  return `${Math.floor(mins / 60)}h ${mins % 60}m`;
}

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

export type ActionsRunSearch = { job?: string };

export type ActionsRunLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      detail: ActionRunGetResponse;
      firstJobLog: ActionJobLogResponse | null;
    };

async function loadActionsRunPage(
  owner: string,
  repoName: string,
  runId: string,
): Promise<ActionsRunLoaderData> {
  const got = await fetchActionsGetRun({ owner, name: repoName, run_id: runId });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load workflow run.",
    };
  }
  const firstJob = got.data.jobs[0];
  let firstJobLog: ActionJobLogResponse | null = null;
  if (firstJob) {
    const log = await fetchActionsGetJobLog({
      owner,
      name: repoName,
      run_id: runId,
      job_id: firstJob.id,
    });
    firstJobLog = log.ok ? log.data : { content: "", next_offset: 0, size: 0 };
  }
  return { kind: "ready", detail: got.data, firstJobLog };
}

function validateSearch(search: Record<string, unknown>): ActionsRunSearch {
  return { job: typeof search.job === "string" ? search.job : undefined };
}

function repoActionsRunQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/actions/$run",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<ActionsRunLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const runId = String((params as { run?: string }).run ?? "");
      try {
        return await loadActionsRunPage(owner, repoName, runId);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Run detail + job logs (ACT-03 / D-ACT-13). Inherits layout chrome (D-QH-01). */
function RepoActionsRunPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/actions/$run", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
    run?: string;
  };
  const owner = String(params.owner ?? "");
  const repoName = String(params.repo ?? "");
  const runId = String(params.run ?? "");
  const search = validateSearch(searchRecord(searchStr)) as ActionsRunSearch;
  const navigate = astroNavigate;
  const routeLoader = useQuery(repoActionsRunQueryOptions(params, searchStr)).data;
  const repo = useRepoStore((s) => s.repo);
  const canWrite = !!repo?.can_write;

  const ssrDetail = routeLoader?.kind === "ready" ? routeLoader.detail : undefined;
  const detailQ = useQuery({
    ...actionsRunDetailQuery(owner, repoName, runId),
    initialData: ssrDetail,
  });
  const jobs = detailQ.data?.jobs ?? ssrDetail?.jobs ?? [];
  const run = detailQ.data?.run ?? ssrDetail?.run;

  // `?job=` permalink — falls back to the first job when missing/unknown.
  const selectedJobId =
    search.job && jobs.some((j) => j.id === search.job) ? search.job : (jobs[0]?.id ?? "");
  const selectedJob = jobs.find((j) => j.id === selectedJobId);
  const ssrFirstJobLog =
    routeLoader?.kind === "ready" &&
    routeLoader.firstJobLog !== null &&
    selectedJobId === routeLoader.detail.jobs[0]?.id
      ? routeLoader.firstJobLog
      : undefined;
  const logQ = useQuery({
    ...actionsJobLogQuery(owner, repoName, runId, selectedJobId),
    enabled: !!selectedJobId,
    initialData: ssrFirstJobLog,
  });
  const logText =
    logQ.data?.content ??
    ssrFirstJobLog?.content ??
    (logQ.isPending ? "Loading log…" : "(empty log)");

  const qc = useQueryClient();
  const rerunM = useMutation({
    ...actionsRerunRunMutation(),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["repo", "actions"] });
    },
  });
  const cancelM = useMutation({
    ...actionsCancelRunMutation(),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ["repo", "actions"] });
    },
  });
  const mutationErr =
    rerunM.isError || cancelM.isError
      ? String(rerunM.error?.message ?? cancelM.error?.message ?? "Action failed")
      : "";

  const runActive = run ? isActiveStatus(run.status) : false;

  function selectJob(id: string) {
    void navigate(
      routeHref("/$owner/$repo/actions/$run", { owner, repo: repoName, run: runId }, { job: id }),
    );
  }

  function rerun(target: { job_id?: string; failed_only?: boolean }) {
    rerunM.mutate({ owner, name: repoName, run_id: runId, ...target });
  }

  // GitHub's meta line: workflow #N · event · ref · commit · actor · timing.
  const firstJobStart = jobs
    .map((j) => j.started_at)
    .filter((t): t is string => !!t)
    .sort()[0];
  const totalSpan = run ? spanDuration(run.created_at, run.finished_at) : "";
  const createdRel =
    run?.created_at && Number.isFinite(Date.parse(run.created_at))
      ? formatRelative(run.created_at)
      : "";
  const runDescription = run
    ? [
        `${run.workflow_name} #${run.run_number}`,
        run.event,
        run.head_ref.replace(/^refs\/(heads|tags)\//, ""),
        run.head_sha.slice(0, 7),
        run.actor ? `by ${run.actor}` : "",
        firstJobStart && run.created_at
          ? `queued ${spanDuration(run.created_at, firstJobStart)}`
          : runActive
            ? "queued"
            : "",
        totalSpan ? `total ${totalSpan}` : "",
        createdRel,
      ]
        .filter(Boolean)
        .join(" · ")
    : "";

  @{
    <div data-testid="repo-actions-run">
      <p className="mb-4">
        <AppLink
          href={`/${owner}/${repoName}/actions` as string}
          className="inline-flex items-center gap-1 text-[13px] text-muted-foreground no-underline hover:text-foreground hover:underline"
        >
          <ArrowLeft aria-hidden size={14} />
          Actions
        </AppLink>
      </p>
      @if (detailQ.isPending && routeLoader?.kind !== "not_found") {
        <>
          <div className="flex flex-col gap-2 border-b border-border pb-4">
            <Skeleton className="h-6 w-1/2" />
            <Skeleton className="h-4 w-40" />
          </div>
          <Skeleton className="mt-8 h-64 w-full" />
        </>;
      } @else {
        @if (!run) {
          <EmptyState
            icon={<Workflow aria-hidden size={22} />}
            title="Run not found"
            description="This workflow run doesn't exist or is no longer available."
          />;
        } @else {
          <>
            <PageHeader
              icon={<Workflow aria-hidden size={22} />}
              title={run.title}
              description={runDescription}
              actions={
                <div className="flex items-center gap-2">
                  <Badge variant={runStatusVariant(run.status)}>{run.status as string}</Badge>
                  {canWrite && runActive ? (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      data-testid="actions-cancel-run"
                      disabled={cancelM.isPending}
                      onClick={() => cancelM.mutate({ owner, name: repoName, run_id: runId })}
                    >
                      <Ban aria-hidden size={14} />
                      Cancel run
                    </Button>
                  ) : undefined}
                  {canWrite && !runActive ? (
                    <ActionsRerunMenu jobs={jobs} busy={rerunM.isPending} onRerun={rerun} />
                  ) : undefined}
                </div>
              }
            />
            @if (mutationErr) {
              <p className="mt-2 text-[13px] text-destructive" role="alert">
                {mutationErr as string}
              </p>;
            }
            <div className="mt-8 grid gap-6 md:grid-cols-[16rem_1fr]">
              <List data-testid="repo-actions-jobs" className="self-start">
                @for (const job of jobs; key job.id) {
                  <ListRow className="p-0">
                    <div className="group flex min-w-0 flex-1 items-center">
                      <button
                        type="button"
                        data-testid={`actions-job-${job.id}`}
                        className={cn(
                          "flex min-w-0 flex-1 items-center gap-2 px-3 py-2 text-left text-[14px] outline-none transition-colors hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
                          job.id === selectedJobId
                            ? "bg-muted/60 font-medium text-foreground"
                            : "text-foreground",
                        )}
                        onClick={() => selectJob(job.id)}
                      >
                        <span className="min-w-0 flex-1">
                          <span className="block truncate">{job.name as string}</span>
                          @if (
                            spanDuration(job.started_at, job.finished_at)
                          ) {
                            <span className="block text-[11px] text-muted-foreground">
                              {
                                spanDuration(
                                  job.started_at,
                                  job.finished_at,
                                ) as string
                              }
                            </span>;
                          }
                        </span>
                        <Badge
                          className="shrink-0"
                          variant={runStatusVariant(job.status)}
                        >
                          {job.status as string}
                        </Badge>
                      </button>
                      @if (
                        canWrite &&
                        !runActive &&
                        !isActiveStatus(job.status)
                      ) {
                        <button
                          type="button"
                          data-testid={`actions-rerun-job-${job.id}`}
                          aria-label={`Re-run ${job.name}`}
                          title="Re-run job"
                          disabled={rerunM.isPending}
                          className="mr-1 shrink-0 rounded p-1.5 text-muted-foreground opacity-0 outline-none transition-opacity hover:bg-muted hover:text-foreground focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring group-hover:opacity-100"
                          onClick={() => rerun({ job_id: job.id })}
                        >
                          <RotateCcw aria-hidden size={13} />
                        </button>;
                      }
                    </div>
                  </ListRow>;
                } @empty {
                  <ListRow>
                    <span className="text-[14px] text-muted-foreground">No jobs.</span>
                  </ListRow>;
                }
              </List>
              <ActionsLogViewer
                log={logText}
                jobStatus={selectedJob?.status ?? ""}
                downloadName={selectedJob?.name || "job"}
              />
            </div>
          </>;
        }
      }
    </div>;
    };
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoActionsRunPage() @{
  <AppPage>
    <RepoLayout>
      <RepoActionsRunPageContent />
    </RepoLayout>
  </AppPage>;
}
