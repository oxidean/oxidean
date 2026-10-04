import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useQuery } from "@octanejs/tanstack-query";
import type { IssueListResponse, LabelPublic, RepoPublic } from "@oxidean/api-client";
import { useState } from "octane";
import {
  ISSUE_LIST_PAGE_SIZE,
  IssuesList,
  type IssueListFilters,
  type IssueListStateFilter,
} from "@/components/repo/issues-list";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { apiClient } from "@/lib/api-client";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";
import { fetchIssueList, fetchLabelListForRepo, fetchRepoGet } from "@/lib/ssr-repo";
import { useChromeAccountState } from "@/lib/use-chrome-account";

export type IssuesLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
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
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }

  const [listRes, labelsRes] = await Promise.all([
    fetchIssueList({
      data: {
        owner,
        name: repoName,
        state,
        offset: 0,
        limit: ISSUE_LIST_PAGE_SIZE,
      },
    }),
    fetchLabelListForRepo({ data: { owner, name: repoName } }),
  ]);

  if (!listRes.ok) {
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
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

export const Route = createFileRoute("/$owner/$repo/issues/")({
  component: IssuesListPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Issues · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<IssuesLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    try {
      return await loadIssuesPage(owner, repoName, "open");
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

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
export function IssuesListPage() @{
  const params = useParams({ strict: false }) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const routeLoader = useLoaderData({ from: "/$owner/$repo/issues/" }) as
    | IssuesLoaderData
    | undefined;
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
    enabled: !!owner && !!repoName,
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
    enabled: !!owner && !!repoName,
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
