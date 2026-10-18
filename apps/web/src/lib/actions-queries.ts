import type {
  ActionArtifactsListResponse,
  ActionDeploymentsListResponse,
  ActionJobLogResponse,
  ActionSecretsListResponse,
  ActionVariablesListResponse,
  AdminWorkflowTemplatesListResponse,
  OrgActionsSecretsListResponse,
  OrgActionsSecretReposResponse,
  OrgActionsVariableReposResponse,
  OrgActionsVariablesListResponse,
  RepoActionsEnvironmentsListResponse,
  WorkflowTemplateGetResponse,
  WorkflowTemplatesListResponse,
} from "@oxidean/api-client";
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
 * Live job log — server-side long-poll: each request passes `wait_ms` so the
 * server holds it until new bytes land or the job goes terminal, then the
 * query re-arms after `refetchInterval`. Effectively a live tail without
 * fixed-cadence polling traffic; on server versions without `wait_ms`
 * support the request returns immediately and the interval keeps things
 * moving. If the stored log shrank (rotation/rewrite), falls back to a full
 * refetch.
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
        wait_ms: 20_000,
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
    // Re-arm quickly after each long-poll response — the server's 20s hold
    // means this only fires when the job is live but silent (or pre-`wait_ms`
    // servers, where it degrades to the old fixed-cadence poll). `done`
    // stops polling entirely on terminal jobs; servers that predate the
    // flag never set it and keep polling.
    refetchInterval: (query: { state: { data?: ActionJobLogResponse } }) =>
      query.state.data?.done ? false : 1_000,
  };
}

export function actionsWorkflowsQuery(owner: string, name: string, gitRef?: string) {
  return actionsListWorkflowsQueryOptions(apiClient, { owner, name, git_ref: gitRef });
}

/** `repo.actions.workflowTemplates.list` — full catalog (server returns all
 * templates when page/per_page are omitted). */
export function actionsWorkflowTemplatesQuery(owner: string, name: string) {
  return {
    queryKey: ["repo", "actions", "workflowTemplates", owner, name] as const,
    queryFn: async (): Promise<WorkflowTemplatesListResponse> => {
      const res = await apiClient.repo.actions.workflowTemplates.list({ owner, name });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    staleTime: 60_000,
  };
}

/** `repo.actions.workflowTemplates.get` — one template's substituted content. */
export function actionsWorkflowTemplateQuery(owner: string, name: string, id: string) {
  return {
    queryKey: ["repo", "actions", "workflowTemplate", owner, name, id] as const,
    queryFn: async (): Promise<WorkflowTemplateGetResponse> => {
      const res = await apiClient.repo.actions.workflowTemplates.get({ owner, name, id });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    enabled: Boolean(id),
    staleTime: 60_000,
  };
}

/** `admin.workflowTemplates.list` — all instance workflow templates. */
export function adminWorkflowTemplatesQuery() {
  return {
    queryKey: ["admin", "workflowTemplates", "list"] as const,
    queryFn: async (): Promise<AdminWorkflowTemplatesListResponse> => {
      const res = await apiClient.admin.workflowTemplates.list();
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
  };
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

/** Run-scoped artifact listing (`repo.actions.listArtifacts`). */
export function actionsArtifactsQuery(owner: string, name: string, runId: string) {
  return {
    queryKey: ["repo", "actions", "artifacts", owner, name, runId] as const,
    queryFn: async (): Promise<ActionArtifactsListResponse> => {
      const res = await apiClient.repo.actions.listArtifacts({ owner, name, run_id: runId });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    enabled: Boolean(runId),
    refetchInterval: 10_000,
  };
}

/** Deployments — run-scoped when `runId` set, repo-level history otherwise. */
export function actionsDeploymentsQuery(
  owner: string,
  name: string,
  opts: { runId?: string; environment?: string } = {},
) {
  return {
    queryKey: [
      "repo",
      "actions",
      "deployments",
      owner,
      name,
      opts.runId ?? "",
      opts.environment ?? "",
    ] as const,
    queryFn: async (): Promise<ActionDeploymentsListResponse> => {
      const res = await apiClient.repo.actions.listDeployments({
        owner,
        name,
        run_id: opts.runId || undefined,
        environment: opts.environment || undefined,
      });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    refetchInterval: 15_000,
  };
}

/** `repo.actions.environments.list`. */
export function actionsEnvironmentsQuery(owner: string, name: string) {
  return {
    queryKey: ["repo", "actions", "environments", owner, name] as const,
    queryFn: async (): Promise<RepoActionsEnvironmentsListResponse> => {
      const res = await apiClient.repo.actions.environments.list({ owner, name });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
  };
}

/** `repo.actions.environments.secretsList` — metadata only. */
export function actionsEnvironmentSecretsQuery(owner: string, name: string, env: string) {
  return {
    queryKey: ["repo", "actions", "env-secrets", owner, name, env] as const,
    queryFn: async (): Promise<ActionSecretsListResponse> => {
      const res = await apiClient.repo.actions.environments.secretsList({
        owner,
        name,
        environment: env,
      });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    enabled: Boolean(env),
  };
}

/** `repo.actions.environments.variablesList` — values are plaintext. */
export function actionsEnvironmentVariablesQuery(owner: string, name: string, env: string) {
  return {
    queryKey: ["repo", "actions", "env-variables", owner, name, env] as const,
    queryFn: async (): Promise<ActionVariablesListResponse> => {
      const res = await apiClient.repo.actions.environments.variablesList({
        owner,
        name,
        environment: env,
      });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    enabled: Boolean(env),
  };
}

/** `org.actions.secrets.list`. */
export function orgActionsSecretsQuery(slug: string) {
  return {
    queryKey: ["org", "actions", "secrets", slug] as const,
    queryFn: async (): Promise<OrgActionsSecretsListResponse> => {
      const res = await apiClient.org.actions.secrets.list({ slug });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    enabled: Boolean(slug),
  };
}

/** `org.actions.secrets.listRepos` — selected-visibility repo ids. */
export function orgActionsSecretReposQuery(slug: string, secret: string) {
  return {
    queryKey: ["org", "actions", "secret-repos", slug, secret] as const,
    queryFn: async (): Promise<OrgActionsSecretReposResponse> => {
      const res = await apiClient.org.actions.secrets.listRepos({ slug, secret_name: secret });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    enabled: Boolean(slug && secret),
  };
}

/** `org.actions.variables.list` — values are plaintext (not secrets). */
export function orgActionsVariablesQuery(slug: string) {
  return {
    queryKey: ["org", "actions", "variables", slug] as const,
    queryFn: async (): Promise<OrgActionsVariablesListResponse> => {
      const res = await apiClient.org.actions.variables.list({ slug });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    enabled: Boolean(slug),
  };
}

/** `org.actions.variables.listRepos` — selected-visibility repo ids. */
export function orgActionsVariableReposQuery(slug: string, variable: string) {
  return {
    queryKey: ["org", "actions", "variable-repos", slug, variable] as const,
    queryFn: async (): Promise<OrgActionsVariableReposResponse> => {
      const res = await apiClient.org.actions.variables.listRepos({
        slug,
        variable_name: variable,
      });
      if (!res.ok) throw new Error(`${res.error.code}: ${res.error.message}`);
      return res.data;
    },
    enabled: Boolean(slug && variable),
  };
}
