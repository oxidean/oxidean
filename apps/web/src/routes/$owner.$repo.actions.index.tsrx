import {
  createFileRoute,
  useLoaderData,
  useNavigate,
  useParams,
  useSearch,
} from "@octanejs/tanstack-router";
import { useMutation, useQuery, useQueryClient } from "@octanejs/tanstack-query";
import { useState } from "octane";
import type {
  ActionRunPublic,
  ActionRunsListResponse,
  ActionWorkflowPublic,
} from "@oxidean/api-client";
import { CheckCircle2, Circle, Loader2, Play, Workflow, XCircle } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import {
  ActionsFiltersBar,
  filtersActive,
  type ActionsFilterDraft,
} from "@/components/repo/actions-filters";
import { useRepoStore } from "@/lib/repo-store";
import { formatRelative } from "@/lib/relative-time";
import { fetchActionsListRuns, fetchActionsListWorkflows, fetchRepoGet } from "@/lib/ssr-repo";
import {
  ACTIONS_RUNS_PER_PAGE,
  actionsDispatchWorkflowMutation,
  actionsRunsQuery,
  actionsWorkflowsQuery,
  type ActionsRunFilters,
} from "@/lib/actions-queries";
import { cn } from "@/lib/utils";

/** Map a workflow run status to a status Badge variant. */
function runStatusVariant(status: string): "success" | "destructive" | "primary" | "default" {
  const s = status.toLowerCase();
  if (s === "success" || s === "completed" || s === "passed") return "success";
  if (s === "failure" || s === "failed" || s === "error" || s === "cancelled") return "destructive";
  if (s === "in_progress" || s === "running" || s === "queued" || s === "pending") return "primary";
  return "default";
}

/** Leading status glyph for a run row. */
function RunStatusIcon({ status }: { status: string }) {
  const v = runStatusVariant(status);
  if (v === "success") {
    return <CheckCircle2 aria-hidden size={18} className="shrink-0 text-green-600" />;
  }
  if (v === "destructive") {
    return <XCircle aria-hidden size={18} className="shrink-0 text-destructive" />;
  }
  if (v === "primary") {
    return <Loader2 aria-hidden size={18} className="shrink-0 animate-spin text-amber-500" />;
  }
  return <Circle aria-hidden size={18} className="shrink-0 text-muted-foreground" />;
}

/** "1m 5s"-style duration between created_at and finished_at (or now). */
function runDuration(run: ActionRunPublic): string {
  if (!run.created_at) return "";
  const start = Date.parse(run.created_at);
  const end = run.finished_at ? Date.parse(run.finished_at) : Date.now();
  if (!Number.isFinite(start) || !Number.isFinite(end) || end < start) return "";
  const secs = Math.floor((end - start) / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ${secs % 60}s`;
  return `${Math.floor(mins / 60)}h ${mins % 60}m`;
}

/** Branch display name for a run's head_ref. */
function shortRef(headRef: string): string {
  return headRef.replace(/^refs\/(heads|tags)\//, "");
}

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

export type ActionsIndexSearch = {
  status?: string;
  event?: string;
  branch?: string;
  workflow?: string;
  actor?: string;
  q?: string;
};

export type ActionsIndexLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      runs: ActionRunsListResponse;
      workflows: ActionWorkflowPublic[] | null;
    };

async function loadActionsIndexPage(
  owner: string,
  repoName: string,
  filters: ActionsIndexSearch,
): Promise<ActionsIndexLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }

  // The workflow sidebar shows for every reader (parity) — dispatch buttons
  // stay gated on can_write below.
  const [runsRes, workflowsRes] = await Promise.all([
    fetchActionsListRuns({
      data: {
        owner,
        name: repoName,
        page: 1,
        per_page: ACTIONS_RUNS_PER_PAGE,
        status: filters.status,
        event: filters.event,
        branch: filters.branch,
        workflow: filters.workflow,
        actor: filters.actor,
        query: filters.q,
      },
    }),
    fetchActionsListWorkflows({ data: { owner, name: repoName } }),
  ]);
  if (!runsRes.ok) {
    if (runsRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: runsRes.error.message || "Could not load workflow runs.",
    };
  }
  return {
    kind: "ready",
    runs: runsRes.data,
    workflows: workflowsRes.ok ? workflowsRes.data.workflows : null,
  };
}

export const Route = createFileRoute("/$owner/$repo/actions/")({
  validateSearch: (search: Record<string, unknown>): ActionsIndexSearch => ({
    status: typeof search.status === "string" ? search.status : undefined,
    event: typeof search.event === "string" ? search.event : undefined,
    branch: typeof search.branch === "string" ? search.branch : undefined,
    workflow: typeof search.workflow === "string" ? search.workflow : undefined,
    actor: typeof search.actor === "string" ? search.actor : undefined,
    q: typeof search.q === "string" ? search.q : undefined,
  }),
  component: RepoActionsPage,
  head: ({ params }) => {
    const p = params as { owner?: string; repo?: string };
    return {
      meta: [
        {
          title: `${String(p.owner ?? "")}/${String(p.repo ?? "")} actions · Oxidean`,
        },
      ],
    };
  },
  loaderDeps: ({ search }): ActionsIndexSearch => ({
    status: search.status,
    event: search.event,
    branch: search.branch,
    workflow: search.workflow,
    actor: search.actor,
    q: search.q,
  }),
  loader: async ({ params, deps }): Promise<ActionsIndexLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    try {
      return await loadActionsIndexPage(owner, repoName, deps);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Workflow runs list — sidebar + filters + run rows (ACT-03 / D-ACT-12, #44). */
export function RepoActionsPage() @{
  const params = useParams({ from: "/$owner/$repo/actions/" }) as {
    owner?: string;
    repo?: string;
  };
  const owner = String(params.owner ?? "");
  const repoName = String(params.repo ?? "");
  const search = useSearch({ strict: false }) as ActionsIndexSearch;
  const navigate = useNavigate();
  const routeLoader = useLoaderData({ from: "/$owner/$repo/actions/" }) as
    | ActionsIndexLoaderData
    | undefined;
  const repo = useRepoStore((s) => s.repo);
  const canWrite = !!repo?.can_write;
  const [page, setPage] = useState(1);
  const [dispatchOpen, setDispatchOpen] = useState(false);
  const [dispatchWorkflow, setDispatchWorkflow] = useState("");
  const [dispatchRef, setDispatchRef] = useState("");
  const [dispatchError, setDispatchError] = useState("");

  const filters: ActionsRunFilters = {
    status: search.status,
    event: search.event,
    branch: search.branch,
    workflow: search.workflow,
    actor: search.actor,
    query: search.q,
  };
  const filterDraft: ActionsFilterDraft = {
    status: search.status ?? "",
    event: search.event ?? "",
    branch: search.branch ?? "",
    actor: search.actor ?? "",
    q: search.q ?? "",
  };
  const hasFilters = filtersActive(filterDraft) || !!search.workflow;

  // The loader already applies the URL filters (loaderDeps), so the SSR
  // payload seeds the filtered first page too.
  const ssrRuns = routeLoader?.kind === "ready" && page === 1 ? routeLoader.runs : undefined;
  const listQ = useQuery({
    ...actionsRunsQuery(owner, repoName, page, filters),
    initialData: ssrRuns,
  });
  const runs = listQ.data?.runs ?? ssrRuns?.runs ?? [];
  const total = listQ.data?.total_count ?? ssrRuns?.total_count ?? 0;
  const loading = listQ.isPending;
  const err =
    routeLoader?.kind === "error"
      ? routeLoader.message
      : listQ.isError
        ? String(listQ.error?.message ?? "Failed to load runs")
        : null;
  const canPrev = page > 1;
  const canNext = page * ACTIONS_RUNS_PER_PAGE < total;

  const workflowsQ = useQuery({
    ...actionsWorkflowsQuery(owner, repoName),
    initialData:
      routeLoader?.kind === "ready" && routeLoader.workflows
        ? { workflows: routeLoader.workflows, git_ref: "" }
        : undefined,
  });
  const workflows =
    workflowsQ.data?.workflows ??
    (routeLoader?.kind === "ready" ? (routeLoader.workflows ?? []) : []);
  const dispatchable = workflows.filter((w) => w.supports_dispatch);

  const qc = useQueryClient();
  const dispatchM = useMutation({
    ...actionsDispatchWorkflowMutation(),
    onSuccess: async () => {
      setDispatchOpen(false);
      setPage(1);
      await qc.invalidateQueries({ queryKey: ["repo", "actions", "listRuns"] });
    },
    onError: (e) => {
      setDispatchError(e instanceof Error ? e.message : "Could not dispatch workflow.");
    },
  });

  function searchFor(next: ActionsIndexSearch): ActionsIndexSearch {
    return {
      status: next.status || undefined,
      event: next.event || undefined,
      branch: next.branch || undefined,
      workflow: next.workflow || undefined,
      actor: next.actor || undefined,
      q: next.q || undefined,
    };
  }

  function goSearch(next: ActionsIndexSearch) {
    setPage(1);
    void navigate({
      to: "/$owner/$repo/actions",
      params: { owner, repo: repoName },
      search: searchFor(next),
    });
  }

  function applyFilters(next: ActionsFilterDraft) {
    // Text/select filters merge onto the sidebar workflow selection.
    goSearch({
      status: next.status,
      event: next.event,
      branch: next.branch,
      actor: next.actor,
      q: next.q,
      workflow: search.workflow,
    });
  }

  function selectWorkflow(path: string) {
    goSearch({ ...search, workflow: path === (search.workflow ?? "") ? "" : path });
  }

  function openDispatch(workflowPath?: string) {
    setDispatchError("");
    setDispatchRef(repo?.default_branch || "main");
    setDispatchWorkflow(workflowPath ?? dispatchable[0]?.path ?? "");
    setDispatchOpen(true);
  }

  function submitDispatch() {
    if (!dispatchWorkflow || !dispatchRef.trim()) return;
    setDispatchError("");
    dispatchM.mutate({
      owner,
      name: repoName,
      workflow_id: dispatchWorkflow,
      git_ref: dispatchRef.trim(),
    });
  }

  @{
    <div data-testid="repo-actions">
      <PageHeader
        icon={<Workflow aria-hidden size={22} />}
        title="Actions"
        description="Workflow runs for this repository."
        actions={
          canWrite && dispatchable.length > 0 ? (
            <Button
              type="button"
              size="sm"
              data-testid="actions-run-workflow"
              onClick={() => openDispatch()}
            >
              <Play aria-hidden size={14} />
              Run workflow
            </Button>
          ) : undefined
        }
      />
      @if (err) {
        <p className="mt-8 text-destructive" role="alert">
          {err as string}
        </p>;
      } @else {
        <div className="mt-6 grid gap-6 md:grid-cols-[14rem_1fr]">
          <nav aria-label="Workflows" data-testid="actions-workflows" className="self-start">
            <button
              type="button"
              className={cn(
                "flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-[14px] outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
                !search.workflow ? "bg-muted/60 font-medium" : "",
              )}
              onClick={() => selectWorkflow("")}
            >
              <Workflow aria-hidden size={15} className="shrink-0 text-muted-foreground" />
              <span className="min-w-0 truncate">All workflows</span>
            </button>
            @for (const w of workflows; key w.path) {
              <div
                className={cn(
                  "group flex items-center rounded-md",
                  search.workflow === w.path ? "bg-muted/60" : "",
                )}
              >
                <button
                  type="button"
                  data-testid={`actions-workflow-${w.path}`}
                  className={cn(
                    "flex min-w-0 flex-1 items-center gap-2 px-3 py-1.5 text-left text-[14px] outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring rounded-md",
                    search.workflow === w.path ? "font-medium" : "",
                  )}
                  onClick={() => selectWorkflow(w.path)}
                >
                  <Workflow
                    aria-hidden
                    size={15}
                    className="shrink-0 text-muted-foreground"
                  />
                  <span className="min-w-0 truncate">{w.name as string}</span>
                </button>
                @if (canWrite && w.supports_dispatch) {
                  <button
                    type="button"
                    data-testid={`actions-dispatch-${w.path}`}
                    aria-label={`Run ${w.name}`}
                    title="Run workflow"
                    className="mr-1 hidden shrink-0 rounded p-1 text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring group-hover:inline-flex"
                    onClick={() => openDispatch(w.path)}
                  >
                    <Play aria-hidden size={13} />
                  </button>;
                }
              </div>;
            }
          </nav>
          <div className="min-w-0">
            <ActionsFiltersBar filters={filterDraft} onApply={applyFilters} />
            @if (hasFilters && !loading) {
              <p className="mt-2 text-[12px] text-muted-foreground">
                {"Filtered: " +
                  [
                    search.workflow ? `workflow ${search.workflow}` : "",
                    search.status ? `status ${search.status}` : "",
                    search.event ? `event ${search.event}` : "",
                    search.branch ? `branch ${search.branch}` : "",
                    search.actor ? `actor ${search.actor}` : "",
                    search.q ? `"${search.q}"` : "",
                  ]
                    .filter(Boolean)
                    .join(" · ")}
              </p>;
            }
            @if (loading) {
              <List className="mt-4">
                @for (const i of [0, 1, 2, 3]; key i) {
                  <ListRow className="gap-3">
                    <Skeleton className="h-4 flex-1" />
                    <Skeleton className="h-5 w-16 rounded-md" />
                  </ListRow>;
                }
              </List>;
            } @else {
              @if (runs.length === 0) {
                @if (hasFilters) {
                  <EmptyState
                    className="mt-6"
                    icon={<Workflow aria-hidden size={22} />}
                    title="No runs match these filters"
                    description="Try broadening the filters or clear them to see all runs."
                  />;
                } @else {
                  <EmptyState
                    className="mt-6"
                    icon={<Workflow aria-hidden size={22} />}
                    title="No workflow runs yet"
                    description="Runs triggered by pushes and other events will appear here."
                  />;
                }
              } @else {
                <>
                  <List className="mt-4" data-testid="repo-actions-run-list">
                    @for (const run of runs; key run.id) {
                      <ListRow className="gap-3">
                        <RunStatusIcon status={run.status} />
                        <div className="min-w-0 flex-1">
                          <a
                            href={
                              `/${owner}/${repoName}/actions/${run.id}` as string
                            }
                            className="block truncate font-medium text-foreground no-underline hover:text-primary hover:underline"
                          >
                            {run.title as string}
                          </a>
                          <p className="mt-0.5 flex flex-wrap items-center gap-x-2 text-[12px] text-muted-foreground">
                            <span>
                              {
                                (run.workflow_name +
                                  " #" +
                                  run.run_number) as string
                              }
                            </span>
                            <span aria-hidden>·</span>
                            <span>{run.event as string}</span>
                            <span aria-hidden>·</span>
                            <span>{shortRef(run.head_ref) as string}</span>
                            <span aria-hidden>·</span>
                            <a
                              href={
                                `/${owner}/${repoName}/commit/${run.head_sha}` as string
                              }
                              className="font-mono text-muted-foreground no-underline hover:text-primary hover:underline"
                            >
                              {run.head_sha.slice(0, 7) as string}
                            </a>
                          </p>
                        </div>
                        <div className="flex shrink-0 flex-col items-end gap-1">
                          <Badge variant={runStatusVariant(run.status)}>
                            {run.status as string}
                          </Badge>
                          <span className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
                            @if (run.actor) {
                              <>
                                @if (
                                  run.actor_avatar_url
                                ) {
                                  <img
                                    src={
                                      run.actor_avatar_url as string
                                    }
                                    alt=""
                                    className="size-4 rounded-full"
                                  />;
                                }
                                <span>{run.actor as string}</span>
                                <span aria-hidden>·</span>
                              </>;
                            }
                            @if (
                              runDuration(run)
                            ) {
                              <>
                                <span>{runDuration(run) as string}</span>
                                <span aria-hidden>·</span>
                              </>;
                            }
                            <span>{formatRelative(run.created_at)}</span>
                          </span>
                        </div>
                      </ListRow>;
                    }
                  </List>
                  @if (canPrev || canNext) {
                    <div className="mt-4 flex items-center justify-between text-[13px]">
                      <span className="text-muted-foreground">
                        {"Page " +
                          page +
                          " · " +
                          total +
                          " run" +
                          (total === 1 ? "" : "s")}
                      </span>
                      <div className="flex gap-2">
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          disabled={!canPrev}
                          onClick={() => setPage(page - 1)}
                        >
                          Newer
                        </Button>
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          disabled={!canNext}
                          onClick={() => setPage(page + 1)}
                        >
                          Older
                        </Button>
                      </div>
                    </div>;
                  }
                </>;
              }
            }
          </div>
        </div>;
      }
      @if (dispatchOpen) {
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4"
          role="presentation"
          data-testid="actions-dispatch-dialog"
        >
          <button
            type="button"
            aria-label="Dismiss"
            className="absolute inset-0 bg-black/40 supports-backdrop-filter:backdrop-blur-[2px]"
            disabled={dispatchM.isPending}
            onClick={() => {
              if (!dispatchM.isPending) setDispatchOpen(false);
            }}
          />
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="actions-dispatch-title"
            className="relative z-10 grid w-full max-w-[calc(100%-2rem)] gap-4 rounded-md border border-border bg-popover p-4 text-[14px] text-popover-foreground shadow-lg outline-none sm:max-w-md"
          >
            <div className="flex flex-col gap-2">
              <h2
                id="actions-dispatch-title"
                className="font-[family-name:var(--font-display)] text-[18px] font-semibold leading-none text-foreground"
              >
                Run workflow
              </h2>
              <p className="text-[14px] text-muted-foreground">
                Manually run a workflow that declares the workflow_dispatch trigger.
              </p>
            </div>
            <div className="flex flex-col gap-2">
              <span className="text-[14px] font-normal leading-[1.4] text-foreground">
                Workflow
              </span>
              <div className="flex flex-col gap-1" role="radiogroup">
                @for (const w of dispatchable; key w.path) {
                  <button
                    type="button"
                    role="radio"
                    aria-checked={dispatchWorkflow === w.path}
                    className={cn(
                      "rounded-md border px-3 py-2 text-left text-[14px] transition-colors",
                      dispatchWorkflow === w.path
                        ? "border-primary bg-primary/5"
                        : "border-border hover:bg-muted/40",
                    )}
                    onClick={() => setDispatchWorkflow(w.path)}
                  >
                    <span className="block font-medium text-foreground">
                      {w.name as string}
                    </span>
                    <span className="block font-mono text-[12px] text-muted-foreground">
                      {w.path as string}
                    </span>
                  </button>;
                }
              </div>
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="actions-dispatch-ref">Branch or tag</Label>
              <Input
                id="actions-dispatch-ref"
                value={dispatchRef}
                onInput={(e) => setDispatchRef((e.target as HTMLInputElement).value)}
                placeholder="main"
              />
            </div>
            <p
              role="alert"
              className={
                dispatchError
                  ? "text-[14px] text-destructive"
                  : "invisible h-0 overflow-hidden text-[14px]"
              }
            >
              {(dispatchError || "\u00a0") as string}
            </p>
            <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
              <Button
                type="button"
                variant="secondary"
                disabled={dispatchM.isPending}
                onClick={() => setDispatchOpen(false)}
              >
                Cancel
              </Button>
              <Button
                type="button"
                disabled={!dispatchWorkflow || !dispatchRef.trim() || dispatchM.isPending}
                onClick={submitDispatch}
              >
                {(dispatchM.isPending ? "Running…" : "Run workflow") as string}
              </Button>
            </div>
          </div>
        </div>;
      }
    </div>;
    };
}
