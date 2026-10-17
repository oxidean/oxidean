import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoPublic, WorkflowTemplatesListResponse } from "@oxidean/api-client";
import { Workflow } from "@octanejs/lucide";
import { AppLink } from "@/components/ui/app-link";
import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";
import { WorkflowTemplateCatalog } from "@/components/repo/workflow-template-catalog";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { actionsWorkflowTemplatesQuery } from "@/lib/actions-queries";
import { fetchActionsWorkflowTemplates, fetchRepoGet } from "@/lib/ssr-repo";

type ActionsNewLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      templates: WorkflowTemplatesListResponse | null;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadActionsNewPage(owner: string, repoName: string): Promise<ActionsNewLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  // Readers still get the page chrome; only writers fetch the catalog.
  if (!got.data.can_write) {
    return { kind: "ready", repo: got.data, templates: null };
  }
  const templatesRes = await fetchActionsWorkflowTemplates({ owner, name: repoName });
  return {
    kind: "ready",
    repo: got.data,
    templates: templatesRes.ok ? templatesRes.data : null,
  };
}

function repoActionsNewQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/actions/new",
      params.owner ?? "",
      params.repo ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<ActionsNewLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      try {
        return await loadActionsNewPage(owner, repoName);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function RepoActionsNewPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/actions/new", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const owner = String(params.owner ?? "");
  const repoName = String(params.repo ?? "");
  const loader = useQuery(repoActionsNewQueryOptions(params, searchStr)).data;

  const repo = loader?.kind === "ready" ? loader.repo : null;
  const canWrite = !!repo?.can_write;

  const templatesQ = useQuery({
    ...actionsWorkflowTemplatesQuery(owner, repoName),
    enabled: canWrite && !!owner && !!repoName,
    initialData: loader?.kind === "ready" && loader.templates ? loader.templates : undefined,
  });
  const catalogError = templatesQ.isError
    ? String(templatesQ.error?.message ?? "Could not load workflow templates.")
    : loader?.kind === "ready" && canWrite && loader.templates == null
      ? "Could not load workflow templates."
      : null;

  @if (loader?.kind === "not_found") {
    <RepoNotFound />;
  } @else {
    <div data-testid="repo-actions-new">
      <PageHeader
        icon={<Workflow aria-hidden size={22} />}
        title="New workflow"
        description="Add a workflow from the catalog or start from scratch."
        actions={
          <AppLink
            href={`/${owner}/${repoName}/actions` as string}
            className="text-[13px] font-medium text-muted-foreground hover:text-foreground"
          >
            {"← Back to Actions" as string}
          </AppLink>
        }
      />
      @if (loader?.kind === "error") {
        <p className="mt-8 text-[14px] text-destructive" role="alert">
          {loader.message as string}
        </p>;
      } @else {
        @if (loader?.kind === "ready" && !canWrite) {
          <EmptyState
            className="mt-8"
            icon={<Workflow aria-hidden size={22} />}
            title="You need write access to add workflows."
            description="Ask a repository maintainer for write access."
          />;
        } @else {
          <div className="mt-6">
            <WorkflowTemplateCatalog
              owner={owner}
              repo={repoName}
              data={templatesQ.data ?? undefined}
              loading={templatesQ.isPending}
              error={catalogError}
            />
          </div>;
        }
      }
    </div>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoActionsNewPage() @{
  <AppPage>
    <RepoLayout>
      <RepoActionsNewPageContent />
    </RepoLayout>
  </AppPage>;
}
