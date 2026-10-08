import { Activity } from "@octanejs/lucide";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoActivityItem, RepoPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { RepoActivityRow } from "@/components/repo/repo-activity-row";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { EmptyState } from "@/components/ui/empty-state";
import { Button } from "@/components/ui/button";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { fetchRepoActivity, fetchRepoGet } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type ActivityLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      items: RepoActivityItem[];
      total: number;
      period: string;
      pushType: string;
      offset: number;
      limit: number;
    };

type Phase =
  | { kind: "loading" }
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      items: RepoActivityItem[];
      total: number;
      period: string;
      pushType: string;
      offset: number;
      limit: number;
      loadingMore: boolean;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const PAGE_LIMIT = 30;

const FILTER_CHIP_ACTIVE =
  "inline-flex items-center rounded-md border border-border bg-muted px-2.5 py-1 text-[12px] font-medium text-foreground no-underline";
const FILTER_CHIP_IDLE =
  "inline-flex items-center rounded-md border border-transparent px-2.5 py-1 text-[12px] text-muted-foreground no-underline hover:bg-muted hover:text-foreground";

const PERIODS = [
  { id: "all", label: "All time" },
  { id: "week", label: "Past week" },
  { id: "month", label: "Past month" },
  { id: "year", label: "Past year" },
] as const;

const TYPES = [
  { id: "all", label: "All activity", value: "" },
  { id: "push", label: "Pushes", value: "push" },
  { id: "force_push", label: "Force pushes", value: "force_push" },
  { id: "pr_merge", label: "Merges", value: "pr_merge" },
  { id: "branch_creation", label: "Branch creates", value: "branch_creation" },
  { id: "branch_deletion", label: "Branch deletes", value: "branch_deletion" },
  { id: "branch_rename", label: "Branch renames", value: "branch_rename" },
] as const;

function activityHrefWithFilters(
  owner: string,
  repo: string,
  period: string,
  pushType: string,
): string {
  const params = new URLSearchParams();
  if (period && period !== "all") params.set("period", period);
  if (pushType) params.set("type", pushType);
  const q = params.toString();
  return `/${owner}/${repo}/activity${q ? `?${q}` : ""}`;
}

function filterChipClass(active: boolean): string {
  return active ? FILTER_CHIP_ACTIVE : FILTER_CHIP_IDLE;
}

async function loadActivityPage(
  owner: string,
  repoName: string,
  period: string,
  pushType: string,
): Promise<ActivityLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return { kind: "error", message: got.error.message || "Could not load repository." };
  }
  const listRes = await fetchRepoActivity({
    owner,
    name: repoName,
    period,
    push_type: pushType || undefined,
    offset: 0,
    limit: PAGE_LIMIT,
  });
  if (!listRes.ok) {
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: listRes.error.message || "Could not load activity.",
    };
  }
  return {
    kind: "ready",
    repo: got.data,
    items: listRes.data.items,
    total: listRes.data.total,
    period,
    pushType,
    offset: listRes.data.offset,
    limit: listRes.data.limit,
  };
}

function toReadyPhase(data: Extract<ActivityLoaderData, { kind: "ready" }>): Phase {
  return { ...data, loadingMore: false };
}

function repoActivityQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/activity",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<ActivityLoaderData> => {
      // loader-deps-ok — searchStr is part of this query's queryKey; search-only navigations refetch via the query.
      const location = { searchStr, pathname: "" };

      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const search = new URLSearchParams(location.searchStr.replace(/^\?/, ""));
      const period = search.get("period") || "all";
      const pushType = search.get("type") || "";
      try {
        return await loadActivityPage(owner, repoName, period, pushType);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Repository activity feed — push / branch / merge events. */
function RepoActivityPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/activity", pathname) ?? {}) as {
    owner: string;
    repo: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const loaderData = useQuery(repoActivityQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() =>
    loaderData
      ? loaderData.kind === "ready"
        ? toReadyPhase(loaderData)
        : loaderData
      : { kind: "loading" },
  );

  const readyDesc =
    phase.kind === "ready"
      ? `${String(phase.total)} event${phase.total === 1 ? "" : "s"} · ${owner}/${repoName}`
      : "";

  async function loadMore() {
    if (phase.kind !== "ready" || phase.loadingMore) return;
    const nextOffset = phase.offset + phase.limit;
    setPhase({ ...phase, loadingMore: true });
    try {
      const list = await apiClient.repo.activityList({
        owner,
        name: repoName,
        period: phase.period,
        push_type: phase.pushType || null,
        offset: nextOffset,
        limit: phase.limit,
      });
      if (!list.ok) {
        setPhase({ ...phase, loadingMore: false });
        return;
      }
      setPhase({
        ...phase,
        items: [...phase.items, ...list.data.items],
        offset: list.data.offset,
        total: list.data.total,
        loadingMore: false,
      });
    } catch {
      setPhase({ ...phase, loadingMore: false });
    }
  }

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <>
      <PageHeader icon={<Activity aria-hidden size={22} />} title="Activity" />
      <List className="mt-6">
        @for (const i of [0, 1, 2, 3, 4]; key i) {
          <ListRow className="gap-3">
            <Skeleton className="h-8 w-8 rounded-full" />
            <Skeleton className="h-4 flex-1" />
            <Skeleton className="h-4 w-16" />
          </ListRow>;
        }
      </List>
    </>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else {
    <>
      <PageHeader
        icon={<Activity aria-hidden size={22} />}
        title="Activity"
        description={readyDesc as string}
      />
      <div className="mt-4 flex flex-wrap gap-2" data-testid="repo-activity-filters">
        @for (const p of PERIODS; key p.id) {
          <AppLink
            href={activityHrefWithFilters(owner, repoName, p.id, phase.pushType) as string}
            className={filterChipClass(p.id === phase.period) as string}
          >
            {p.label as string}
          </AppLink>;
        }
        @for (const t of TYPES; key t.id) {
          <AppLink
            href={activityHrefWithFilters(owner, repoName, phase.period, t.value) as string}
            className={filterChipClass(t.value === phase.pushType) as string}
          >
            {t.label as string}
          </AppLink>;
        }
      </div>
      @if (phase.items.length === 0) {
        <EmptyState
          className="mt-8"
          icon={<Activity aria-hidden size={22} />}
          title="No activity yet"
          description="Pushes, branch changes, and merges will show up here."
        />;
      } @else {
        <List className="mt-6" data-testid="repo-activity-list">
          @for (const item of phase.items; key item.id) {
            <RepoActivityRow owner={owner} repoName={repoName} item={item} />;
          }
        </List>;
      }
      @if (
        phase.items.length > 0 &&
        phase.items.length < phase.total
      ) {
        <div className="mt-4 flex justify-center">
          <Button
            type="button"
            variant="secondary"
            disabled={phase.loadingMore}
            onClick={() => void loadMore()}
          >
            {(phase.loadingMore ? "Loading…" : "Load more") as string}
          </Button>
        </div>;
      }
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoActivityPage() @{
  <AppPage>
    <RepoLayout>
      <RepoActivityPageContent />
    </RepoLayout>
  </AppPage>;
}
