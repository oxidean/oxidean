import { useQuery } from "@octanejs/tanstack-query";
import type {
  CheckRunAnnotationPublic,
  CheckRunPublic,
  CheckSuitePublic,
} from "@oxidean/api-client";
import {
  CheckCircle2,
  Circle,
  CircleDot,
  ListChecks,
  Loader2,
  MinusCircle,
  XCircle,
} from "@octanejs/lucide";
import { useState } from "octane";
import { AppLink } from "@/components/ui/app-link";
import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/ui/empty-state";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { cn } from "@/lib/utils";

type PullV2ChecksProps = {
  owner: string;
  repoName: string;
  /** Head SHA whose checks gate the merge (PR head commit). */
  sha: string;
};

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

/** Suite/run status+conclusion folded onto the commit-status vocabulary. */
function checkState(status: string, conclusion?: string | null): string {
  if (status === "completed") return conclusion ?? "failure";
  return "pending";
}

function CheckStateIcon(props: { state: string }) @{
  @if (props.state === "success") {
    <CheckCircle2 aria-hidden size={16} className="shrink-0 text-green-600" />;
  } @else {
    @if (
      props.state === "failure" ||
      props.state === "error" ||
      props.state === "timed_out" ||
      props.state === "startup_failure"
    ) {
      <XCircle aria-hidden size={16} className="shrink-0 text-destructive" />;
    } @else {
      @if (
        props.state === "neutral" ||
        props.state === "skipped" ||
        props.state === "stale"
      ) {
        <MinusCircle aria-hidden size={16} className="shrink-0 text-muted-foreground" />;
      } @else {
        @if (props.state === "pending") {
          <Loader2 aria-hidden size={16} className="shrink-0 animate-spin text-amber-500" />;
        } @else {
          <Circle aria-hidden size={16} className="shrink-0 text-muted-foreground" />;
        }
      }
    }
  }
}

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

function suiteLabel(suite: CheckSuitePublic): string {
  return suite.app === "github-actions" ? "GitHub Actions" : suite.app;
}

function isAuthError(error: unknown): boolean {
  return error instanceof Error && error.message.startsWith("auth.");
}

function annotationVariant(level: string): "destructive" | "primary" | "default" {
  if (level === "failure") return "destructive";
  if (level === "warning") return "primary";
  return "default";
}

/** Run detail pane — meta + output + annotations list (S10). */
function PullV2CheckRunDetail(props: {
  owner: string;
  repoName: string;
  run: CheckRunPublic;
}) @{
  // The suite list carries summary runs — fetch the full record so the
  // output text/detail fields populate.
  const runQ = useQuery({
    queryKey: ["check", "runs", "get", props.owner, props.repoName, props.run.id] as const,
    queryFn: async () => {
      const res = await apiClient.check.runs.get({
        owner: props.owner,
        name: props.repoName,
        id: props.run.id,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });
  const annotationsQ = useQuery({
    queryKey: ["check", "annotations", "list", props.owner, props.repoName, props.run.id] as const,
    queryFn: async () => {
      const res = await apiClient.check.annotations.list({
        owner: props.owner,
        name: props.repoName,
        check_run_id: props.run.id,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
  });
  const annotations: CheckRunAnnotationPublic[] = annotationsQ.data?.annotations ?? [];
  const run = runQ.data ?? props.run;
  const state = checkState(run.status, run.conclusion);

  <div className="space-y-3" data-testid="pull-v2-check-detail">
    <div className="flex flex-wrap items-center gap-2">
      <CheckStateIcon state={state} />
      <h3 className="text-[14px] font-semibold text-foreground">{run.name as string}</h3>
      <Badge variant={checkStateVariant(state)}>{state as string}</Badge>
      @if (run.run_id) {
        <AppLink
          href={`/${props.owner}/${props.repoName}/actions/${run.run_id}` as string}
          className="ml-auto text-[13px] text-primary no-underline hover:underline"
        >
          View run
        </AppLink>;
      }
    </div>
    <p className="text-[12px] text-muted-foreground">
      {
        `${run.app === "github-actions" ? "GitHub Actions" : run.app} · updated ${formatRelative(run.updated_at)}` as string
      }
    </p>
    @if (run.output?.title) {
      <p className="text-[13px] font-medium text-foreground">{run.output.title as string}</p>;
    }
    @if (run.output?.summary) {
      <pre className="whitespace-pre-wrap rounded-md border border-border bg-muted/30 p-3 font-sans text-[13px] text-foreground">
        {run.output.summary as string}
      </pre>;
    }
    @if (run.output?.text) {
      <pre className="whitespace-pre-wrap rounded-md border border-border bg-muted/30 p-3 font-mono text-[12px] text-foreground">
        {run.output.text as string}
      </pre>;
    }
    @if (annotationsQ.isPending) {
      <p className="text-[13px] text-muted-foreground">Loading annotations…</p>;
    } @else {
      @if (annotations.length > 0) {
        <ul className="divide-y divide-border rounded-md border border-border">
          @for (const a of annotations; key a.id) {
            <li
              className="px-3 py-2 text-[13px]"
              data-testid="pull-v2-check-annotation"
            >
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant={annotationVariant(a.annotation_level)}>
                  {a.annotation_level as string}
                </Badge>
                <span className="font-mono text-[12px] text-muted-foreground">
                  {
                    `${a.path}:${a.start_line}${a.end_line && a.end_line !== a.start_line ? `-${a.end_line}` : ""}` as string
                  }
                </span>
                @if (a.title) {
                  <span className="font-medium text-foreground">
                    {a.title as string}
                  </span>;
                }
              </div>
              <p className="mt-1 whitespace-pre-wrap text-foreground">
                {a.message as string}
              </p>
            </li>;
          }
        </ul>;
      }
    }
  </div>;
}

/**
 * Checks tab of the new PR experience (WI-0001 S10): GitHub-style suites
 * sidebar on the left — pick a suite to list its runs, pick a run for
 * details + annotations.
 */
export function PullV2Checks(props: PullV2ChecksProps) @{
  const [selectedSuiteId, setSelectedSuiteId] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);

  const suitesQ = useQuery({
    queryKey: ["check", "suites", "list", props.owner, props.repoName, props.sha] as const,
    queryFn: async () => {
      const res = await apiClient.check.suites.list({
        owner: props.owner,
        name: props.repoName,
        head_sha: props.sha,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!props.sha,
    refetchInterval: 10_000,
  });

  const suites: CheckSuitePublic[] = suitesQ.data?.suites ?? [];
  const activeSuite = suites.find((s) => s.id === selectedSuiteId) ?? suites[0] ?? null;

  const runsQ = useQuery({
    queryKey: [
      "check",
      "runs",
      "listForSuite",
      props.owner,
      props.repoName,
      activeSuite?.id ?? "",
    ] as const,
    queryFn: async () => {
      const res = await apiClient.check.runs.listForSuite({
        owner: props.owner,
        name: props.repoName,
        suite_id: activeSuite?.id ?? "",
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!activeSuite,
    refetchInterval: 10_000,
  });

  const runs: CheckRunPublic[] = runsQ.data?.check_runs ?? [];
  const activeRun = runs.find((r) => r.id === selectedRunId) ?? runs[0] ?? null;

  <div data-testid="pull-v2-checks">
    @if (suitesQ.isPending) {
      <p className="text-sm text-muted-foreground">Loading checks…</p>;
    } @else {
      @if (suitesQ.isError) {
        @if (isAuthError(suitesQ.error)) {
          <p className="text-sm text-muted-foreground" data-testid="pull-v2-checks-signin">
            Sign in to view checks for this pull request.
          </p>;
        } @else {
          <p role="alert" className="text-sm text-destructive">
            Could not load checks.
          </p>;
        }
      } @else {
        @if (suites.length === 0) {
          <EmptyState
            compact
            icon={<ListChecks aria-hidden size={20} />}
            title="No checks"
            description="No check suites have been reported for this commit yet."
          />;
        } @else {
          <div className="flex items-start gap-4">
            <aside className="w-60 shrink-0 rounded-md border border-border bg-card">
              <p className="border-b border-border px-3 py-2 text-[12px] font-semibold text-muted-foreground">
                {
                  `${suites.length} check suite${suites.length === 1 ? "" : "s"} on ${props.sha.slice(0, 7)}` as string
                }
              </p>
              <ul className="divide-y divide-border" data-testid="pull-v2-check-suites">
                @for (const s of suites; key s.id) {
                  <li>
                    <button
                      type="button"
                      className={
                        cn(
                          "flex w-full items-center gap-2 px-3 py-2 text-left text-[13px]",
                          activeSuite?.id === s.id
                            ? "bg-primary/10 font-medium text-foreground"
                            : "text-foreground hover:bg-muted/50",
                        ) as string
                      }
                      aria-current={activeSuite?.id === s.id}
                      data-testid="pull-v2-check-suite"
                      onClick={() => {
                        setSelectedSuiteId(s.id);
                        setSelectedRunId(null);
                      }}
                    >
                      <CheckStateIcon state={checkState(s.status, s.conclusion)} />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate">
                          {suiteLabel(s) as string}
                        </span>
                        <span className="block text-[11px] text-muted-foreground">
                          {
                            `${s.check_runs_count} run${s.check_runs_count === 1 ? "" : "s"}` as string
                          }
                        </span>
                      </span>
                    </button>
                  </li>;
                }
              </ul>
            </aside>
            <div className="min-w-0 flex-1 space-y-3">
              @if (activeSuite) {
                <>
                  @if (runs.length > 0) {
                    <ul
                      className="flex flex-wrap gap-1"
                      role="tablist"
                      aria-label="Check runs"
                      data-testid="pull-v2-check-runs"
                    >
                      @for (const r of runs; key r.id) {
                        <li>
                          <button
                            type="button"
                            role="tab"
                            aria-selected={activeRun?.id === r.id}
                            className={
                              cn(
                                "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[12px]",
                                activeRun?.id === r.id
                                  ? "border-primary/40 bg-primary/10 font-medium text-foreground"
                                  : "border-border text-muted-foreground hover:bg-muted/50",
                              ) as string
                            }
                            data-testid="pull-v2-check-run"
                            onClick={() => setSelectedRunId(r.id)}
                          >
                            @if (
                              checkState(r.status, r.conclusion) ===
                              "pending"
                            ) {
                              <CircleDot aria-hidden size={12} />;
                            } @else {
                              <CheckStateIcon
                                state={checkState(
                                  r.status,
                                  r.conclusion,
                                )}
                              />;
                            }
                            {r.name as string}
                          </button>
                        </li>;
                      }
                    </ul>;
                  }
                  @if (runsQ.isPending) {
                    <p className="text-[13px] text-muted-foreground">
                      Loading runs…
                    </p>;
                  } @else {
                    @if (runs.length === 0) {
                      <p className="text-[13px] text-muted-foreground">
                        No check runs in this suite yet.
                      </p>;
                    } @else {
                      @if (activeRun) {
                        <PullV2CheckRunDetail
                          owner={props.owner}
                          repoName={props.repoName}
                          run={activeRun}
                        />;
                      }
                    }
                  }
                </>;
              }
            </div>
          </div>;
        }
      }
    }
  </div>;
}
