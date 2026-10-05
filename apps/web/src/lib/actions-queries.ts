import type { ActionJobLogResponse } from "@oxidean/api-client";
import type { QueryFunctionContext } from "@octanejs/tanstack-query";
import {
  actionsCancelRunMutationOptions,
  actionsDispatchWorkflowMutationOptions,
  actionsGetJobLogQueryOptions,
  actionsGetRunQueryOptions,
  actionsListRunsQueryOptions,
  actionsListWorkflowsQueryOptions,
  actionsRerunRunMutationOptions,
} from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";

export const ACTIONS_RUNS_PER_PAGE = 25;

/** Server-side run-list filters (issue #44 — Actions parity). */
export type ActionsRunFilters = {
  status?: string;
  event?: string;
  branch?: string;
  workflow?: string;
  actor?: string;
  query?: string;
};

export function actionsRunsQuery(
  owner: string,
  name: string,
  page = 1,
  filters: ActionsRunFilters = {},
) {
  return {
    ...actionsListRunsQueryOptions(apiClient, {
      owner,
      name,
      page,
      per_page: ACTIONS_RUNS_PER_PAGE,
      status: filters.status || undefined,
      event: filters.event || undefined,
      branch: filters.branch || undefined,
      workflow: filters.workflow || undefined,
      actor: filters.actor || undefined,
      query: filters.query || undefined,
    }),
    refetchInterval: 10_000,
  };
}

export function actionsRunDetailQuery(owner: string, name: string, runId: string) {
  return {
    ...actionsGetRunQueryOptions(apiClient, { owner, name, run_id: runId }),
    refetchInterval: 5_000,
  };
}

/**
 * Live job log — refetches every 5s, requesting only bytes after the
 * previously seen `next_offset` and appending the tail. If the stored log
 * shrank (rotation/rewrite), falls back to a full refetch.
 */
export function actionsJobLogQuery(owner: string, name: string, runId: string, jobId: string) {
  const base = actionsGetJobLogQueryOptions(apiClient, {
    owner,
    name,
    run_id: runId,
    job_id: jobId,
  });
  return {
    ...base,
    queryFn: async (ctx: QueryFunctionContext) => {
      const prev = ctx.client.getQueryData(ctx.queryKey) as ActionJobLogResponse | undefined;
      const offset = prev?.next_offset ?? 0;
      const res = await apiClient.repo.actions.getJobLog({
        owner,
        name,
        run_id: runId,
        job_id: jobId,
        offset,
      });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      const data = res.data;
      if (prev && offset > 0) {
        if (data.size < offset) {
          // Log shrank server-side — restart from the beginning.
          const full = await apiClient.repo.actions.getJobLog({
            owner,
            name,
            run_id: runId,
            job_id: jobId,
          });
          if (!full.ok) throw new Error(`${full.error.code}: ${full.error.message}`);
          return full.data;
        }
        // A dangling U+FFFD means a multi-byte char straddled the old EOF; the
        // snapped-forward offset already skipped its tail — drop the marker.
        const prevText = prev.content.endsWith("\u{FFFD}")
          ? prev.content.slice(0, -1)
          : prev.content;
        return { ...data, content: prevText + data.content };
      }
      return data;
    },
    refetchInterval: 5_000,
  };
}

export function actionsWorkflowsQuery(owner: string, name: string, gitRef?: string) {
  return actionsListWorkflowsQueryOptions(apiClient, { owner, name, git_ref: gitRef });
}

export function actionsDispatchWorkflowMutation() {
  return actionsDispatchWorkflowMutationOptions(apiClient);
}

export function actionsRerunRunMutation() {
  return actionsRerunRunMutationOptions(apiClient);
}

export function actionsCancelRunMutation() {
  return actionsCancelRunMutationOptions(apiClient);
}
