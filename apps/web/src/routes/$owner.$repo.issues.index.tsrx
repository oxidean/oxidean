import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { useRepoLayoutData } from "@/lib/repo-store.tsrx";
import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { IssueListResponse, LabelPublic, RepoPublic } from "@oxidean/api-client";
import { useState } from "octane";
import {
  ISSUE_LIST_PAGE_SIZE,
  IssuesList,
  type IssueListFilters,
  type IssueListStateFilter,
} from "@/components/repo/issues-list";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { apiClient } from "@/lib/api-client";

import { fetchIssueList, fetchLabelListForRepo, fetchRepoGet } from "@/lib/ssr-repo";
import { useChromeAccountState } from "@/lib/use-chrome-account";

export type IssuesLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  // Issues unit disabled for this repo (COL-13).
  | { kind: "disabled"; repo: RepoPublic }
  | {
      kind: "ready";
      repo: RepoPublic;
      list: IssueListResponse;
      labels: LabelPublic[];
      state: IssueListStateFilter;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadIssuesPage(
  owner: string,
  repoName: string,
  state: IssueListStateFilter,
): Promise<IssuesLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }

  // Unit toggle (COL-13): skip unit RPCs when the repo says Issues is off.
  if (got.data.issues_enabled === false) {
    return { kind: "disabled", repo: got.data };
  }

  const [listRes, labelsRes] = await Promise.all([
    fetchIssueList({
      owner,
      name: repoName,
      state,
      offset: 0,
      limit: ISSUE_LIST_PAGE_SIZE,
    }),
    fetchLabelListForRepo({ owner, name: repoName }),
  ]);

  if (!listRes.ok) {
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
    // Toggled off between the repo.get and the list call.
    if (listRes.error.code === "repo.issues.disabled") {
      return { kind: "disabled", repo: got.data };
    }
    return {
      kind: "error",
      message: listRes.error.message || "Could not load issues.",
    };
  }

  const labels = labelsRes.ok ? labelsRes.data.labels : [];

  return {
    kind: "ready",
    repo: got.data,
    list: listRes.data,
    labels,
    state,
  };
}

function repoIssuesIndexQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/issues/",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<IssuesLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      try {
        return await loadIssuesPage(owner, repoName, "open");
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function emptyFilters(state: IssueListStateFilter = "open"): IssueListFilters {
  return {
    state,
    author: "",
    label: "",
    assignee: "",
    q: "",
    offset: 0,
    limit: ISSUE_LIST_PAGE_SIZE,
  };
}

/** Issues list — Open default + filters + offset pages (D-ISS-16..19). */
function IssuesListPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/issues/", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useRepoLayoutData();
  const routeLoader = useQuery(repoIssuesIndexQueryOptions(params, searchStr)).data;
  const account = useChromeAccountState();

  const [filters, setFilters] = useState<IssueListFilters>(() => emptyFilters("open"));

  const repoQ = useQuery({
    queryKey: ["repo", "get", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.get({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName,
    initialData:
      routeLoader?.kind === "ready"
        ? routeLoader.repo
        : layout?.status === "ok"
          ? (layout.repo ?? undefined)
          : undefined,
  });

  // Issues unit off (COL-13): loader flag, live repo.get flag, or the loader's
  // own repo payload (layout fallback may carry a list-defaulted flag).
  const unitDisabled =
    routeLoader?.kind === "disabled" ||
    (routeLoader?.kind === "ready" ? routeLoader.repo : layout?.repo)?.issues_enabled === false ||
    repoQ.data?.issues_enabled === false;

  const labelsQ = useQuery({
    queryKey: ["label", "listForRepo", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.label.listForRepo({
        owner,
        name: repoName,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.labels;
    },
    enabled: !!owner && !!repoName && !unitDisabled,
    initialData: routeLoader?.kind === "ready" ? routeLoader.labels : undefined,
  });

  const ssrListMatches =
    routeLoader?.kind === "ready" &&
    filters.state === "open" &&
    !filters.author &&
    !filters.label &&
    !filters.assignee &&
    !filters.q &&
    filters.offset === 0;

  const listQ = useQuery({
    queryKey: [
      "issue",
      "list",
      owner,
      repoName,
      filters.state,
      filters.author,
      filters.label,
      filters.assignee,
      filters.q,
      filters.offset,
      filters.limit,
    ] as const,
    queryFn: async () => {
      const res = await apiClient.issue.list({
        owner,
        name: repoName,
        state: filters.state,
        author: filters.author || null,
        label: filters.label || null,
        assignee: filters.assignee || null,
        q: filters.q || null,
        offset: filters.offset,
        limit: filters.limit,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName && !unitDisabled,
    initialData: ssrListMatches ? routeLoader.list : undefined,
  });

  const repo =
    repoQ.data ??
    (routeLoader?.kind === "ready" ? routeLoader.repo : null) ??
    (layout?.status === "ok" ? layout.repo : null) ??
    null;
  const list = listQ.data ?? (ssrListMatches ? routeLoader.list : undefined);
  const labels =
    labelsQ.data ?? (routeLoader?.kind === "ready" ? routeLoader.labels : undefined) ?? [];
  const notFound =
    layout?.status === "not_found" ||
    routeLoader?.kind === "not_found" ||
    (repoQ.isError && String(repoQ.error?.message ?? "").includes("not found"));

  const applyFilters = (draft: { author: string; label: string; assignee: string; q: string }) => {
    setFilters((prev) => ({
      ...prev,
      author: draft.author,
      label: draft.label,
      assignee: draft.assignee,
      q: draft.q,
      offset: 0,
    }));
  };

  const setState = (next: IssueListStateFilter) => {
    setFilters((prev) => ({ ...prev, state: next, offset: 0 }));
  };

  const prevPage = () => {
    setFilters((prev) => ({
      ...prev,
      offset: Math.max(0, prev.offset - prev.limit),
    }));
  };

  const nextPage = () => {
    setFilters((prev) => ({
      ...prev,
      offset: prev.offset + prev.limit,
    }));
  };

  @if (notFound || (!repoQ.isPending && !repo && routeLoader?.kind !== "ready")) {
    <RepoNotFound />;
  } @else if (unitDisabled) {
    <RepoUnitDisabled unit="issues" canAdmin={repo?.can_admin ?? false} />;
  } @else if (!repo) {
    <p className="text-[16px] text-muted-foreground">Loading issues…</p>;
  } @else {
    @if (listQ.isError && !list) {
      <p role="alert" className="text-[16px] text-destructive">
        {(listQ.error?.message || NETWORK_ERROR) as string}
      </p>;
    } @else {
      <IssuesList
        owner={owner}
        repoName={repoName}
        repo={repo}
        canInteract={!!account.user?.email_verified}
        issues={list?.issues ?? []}
        total={list?.total ?? 0}
        filters={filters}
        labels={labels}
        onStateChange={setState}
        onApplyFilters={applyFilters}
        onPrevPage={prevPage}
        onNextPage={nextPage}
        pending={listQ.isPending && !list}
      />;
    }
  }
}

export default IssuesListPage;

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function IssuesListPage() @{
  <AppPage>
    <RepoLayout>
      <IssuesListPageContent />
    </RepoLayout>
  </AppPage>;
}
