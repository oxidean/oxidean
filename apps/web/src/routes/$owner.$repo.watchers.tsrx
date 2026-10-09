import { Eye, Search } from "@octanejs/lucide";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { useRepoLayoutData } from "@/lib/repo-store.tsrx";
import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoPublic, RepoWatcherPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { EmptyState } from "@/components/ui/empty-state";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { apiClient } from "@/lib/api-client";
import { socialListHref } from "@/lib/repo-social-lists";

import { fetchRepoGet, fetchRepoWatchers } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

type WatchersLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      watchers: RepoWatcherPublic[];
      total: number;
      q: string;
      offset: number;
      limit: number;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const PAGE_LIMIT = 50;

async function loadWatchersPage(
  owner: string,
  repoName: string,
  q: string,
): Promise<WatchersLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return { kind: "error", message: got.error.message || "Could not load repository." };
  }
  const listRes = await fetchRepoWatchers({
    owner,
    name: repoName,
    q,
    offset: 0,
    limit: PAGE_LIMIT,
  });
  if (!listRes.ok) {
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: listRes.error.message || "Could not load watchers.",
    };
  }
  return {
    kind: "ready",
    repo: got.data,
    watchers: listRes.data.watchers,
    total: listRes.data.total,
    q,
    offset: 0,
    limit: PAGE_LIMIT,
  };
}

function repoWatchersQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/watchers",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<WatchersLoaderData> => {
      // loader-deps-ok — searchStr is part of this query's queryKey; search-only navigations refetch via the query.
      const location = { searchStr, pathname: "" };

      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const q = new URLSearchParams(location.searchStr.replace(/^\?/, "")).get("q") ?? "";
      try {
        return await loadWatchersPage(owner, repoName, q);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function RepoWatchersPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/watchers", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useRepoLayoutData();
  const data = useQuery(repoWatchersQueryOptions(params, searchStr)).data;

  const showNotFound = !data || data.kind === "not_found" || layout?.status === "not_found";
  const errorMessage = data?.kind === "error" ? data.message : "";
  const ready = data?.kind === "ready" ? data : null;
  const [q, setQ] = useState(ready?.q ?? "");
  const [watchers, setWatchers] = useState(ready?.watchers ?? []);
  const [offset, setOffset] = useState(ready?.offset ?? 0);
  const [total, setTotal] = useState(ready?.total ?? 0);
  const [loadingMore, setLoadingMore] = useState(false);
  const limit = ready?.limit ?? PAGE_LIMIT;
  const base = `/${owner}/${repoName}/watchers`;
  const activeQ = ready?.q ?? "";
  const emptyTitle = activeQ ? "No matching watchers" : "There are no watchers yet";
  const emptyDesc = activeQ
    ? "Try a different username or clear the search."
    : "People who watch this repository will appear here.";
  const desc = `${String(total)} watching ${owner}/${repoName}`;

  async function loadMore() {
    if (loadingMore || watchers.length >= total) return;
    const nextOffset = offset + limit;
    setLoadingMore(true);
    try {
      const list = await apiClient.repo.watchersList({
        owner,
        name: repoName,
        q: activeQ || null,
        offset: nextOffset,
        limit,
      });
      if (!list.ok) {
        setLoadingMore(false);
        return;
      }
      setWatchers([...watchers, ...list.data.watchers]);
      setOffset(nextOffset);
      setTotal(list.data.total);
      setLoadingMore(false);
    } catch {
      setLoadingMore(false);
    }
  }

  @if (showNotFound) {
    <RepoNotFound />;
  } @else {
    @if (errorMessage) {
      <p className="text-muted-foreground" role="alert">
        {errorMessage as string}
      </p>;
    } @else {
      <div data-testid="repo-watchers">
        <PageHeader
          icon={<Eye aria-hidden size={22} />}
          title="Watchers"
          description={desc as string}
          actions={
            <form
              method="get"
              action={base as string}
              className="flex w-full gap-2 sm:w-auto"
              onSubmit={(e) => {
                e.preventDefault();
                window.location.assign(socialListHref(base, { q }));
              }}
            >
              <div className="relative flex-1 sm:w-72">
                <Search
                  aria-hidden
                  size={16}
                  className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
                />
                <Input
                  name="q"
                  value={q as string}
                  onInput={(e) => setQ((e.currentTarget as HTMLInputElement).value)}
                  placeholder="Find a watcher"
                  className="pl-8"
                  aria-label="Find a watcher"
                  autoComplete="off"
                />
              </div>
            </form>
          }
        />
        @if (watchers.length === 0) {
          <EmptyState
            className="mt-8"
            icon={<Eye aria-hidden size={22} />}
            title={emptyTitle as string}
            description={emptyDesc as string}
          />;
        } @else {
          <List className="mt-6" aria-label="Watchers">
            @for (const w of watchers; key w.user_id) {
              <ListRow className="flex items-center gap-3 py-3">
                <AppLink
                  href={("/" + w.username) as string}
                  className="inline-flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-full border border-border bg-muted text-[12px] font-semibold text-foreground no-underline"
                >
                  @if (w.avatar_url) {
                    <img
                      src={w.avatar_url as string}
                      alt=""
                      className="h-full w-full object-cover"
                    />;
                  } @else {
                    <span>
                      {w.display_name.slice(0, 1).toUpperCase() as string}
                    </span>;
                  }
                </AppLink>
                <div className="min-w-0 flex-1">
                  <AppLink
                    href={("/" + w.username) as string}
                    className="font-medium text-foreground no-underline hover:underline"
                  >
                    {w.username as string}
                  </AppLink>
                  @if (
                    w.display_name &&
                    w.display_name !== w.username
                  ) {
                    <span className="ml-1.5 text-[13px] text-muted-foreground">
                      {w.display_name as string}
                    </span>;
                  }
                </div>
              </ListRow>;
            }
          </List>;
        }
        @if (watchers.length > 0 && watchers.length < total) {
          <div className="mt-4 flex justify-center">
            <Button
              type="button"
              variant="secondary"
              disabled={loadingMore}
              onClick={() => void loadMore()}
            >
              {(loadingMore ? "Loading…" : "Load more") as string}
            </Button>
          </div>;
        }
      </div>;
    }
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoWatchersPage() @{
  <AppPage>
    <RepoLayout>
      <RepoWatchersPageContent />
    </RepoLayout>
  </AppPage>;
}
