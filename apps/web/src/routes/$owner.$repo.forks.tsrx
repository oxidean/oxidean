import { navigate } from "astro:transitions/client";
import { GitFork, Search, Star } from "@octanejs/lucide";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { useRepoLayoutData } from "@/lib/repo-store.tsrx";
import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoForkPublic, RepoPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { EmptyState } from "@/components/ui/empty-state";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { apiClient } from "@/lib/api-client";
import {
  forksSortLabel,
  parseForksSort,
  socialListHref,
  type ForksSortOption,
} from "@/lib/repo-social-lists";
import { formatRelativeUpdated } from "@/lib/relative-time";

import { fetchRepoForks, fetchRepoGet } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

type ForksLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      forks: RepoForkPublic[];
      total: number;
      q: string;
      sort: ForksSortOption;
      offset: number;
      limit: number;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const PAGE_LIMIT = 50;

async function loadForksPage(
  owner: string,
  repoName: string,
  q: string,
  sort: ForksSortOption,
): Promise<ForksLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return { kind: "error", message: got.error.message || "Could not load repository." };
  }
  const listRes = await fetchRepoForks({
    owner,
    name: repoName,
    q,
    sort,
    offset: 0,
    limit: PAGE_LIMIT,
  });
  if (!listRes.ok) {
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: listRes.error.message || "Could not load forks.",
    };
  }
  return {
    kind: "ready",
    repo: got.data,
    forks: listRes.data.forks,
    total: listRes.data.total,
    q,
    sort,
    offset: 0,
    limit: PAGE_LIMIT,
  };
}

function repoForksQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/forks",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<ForksLoaderData> => {
      // loader-deps-ok — searchStr is part of this query's queryKey; search-only navigations refetch via the query.
      const location = { searchStr, pathname: "" };

      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const sp = new URLSearchParams(location.searchStr.replace(/^\?/, ""));
      const q = sp.get("q") ?? "";
      const sort = parseForksSort(sp.get("sort"));
      try {
        return await loadForksPage(owner, repoName, q, sort);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function RepoForksPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/forks", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useRepoLayoutData();
  const data = useQuery(repoForksQueryOptions(params, searchStr)).data;

  const showNotFound = !data || data.kind === "not_found" || layout?.status === "not_found";
  const errorMessage = data?.kind === "error" ? data.message : "";
  const ready = data?.kind === "ready" ? data : null;
  const [q, setQ] = useState(ready?.q ?? "");
  const [sort, setSort] = useState<ForksSortOption>(ready?.sort ?? "stars");
  const [forks, setForks] = useState(ready?.forks ?? []);
  const [offset, setOffset] = useState(ready?.offset ?? 0);
  const [total, setTotal] = useState(ready?.total ?? 0);
  const [loadingMore, setLoadingMore] = useState(false);

  useEffect(() => {
    if (!ready) return;
    setQ(ready.q);
    setSort(ready.sort);
    setForks(ready.forks);
    setOffset(ready.offset);
    setTotal(ready.total);
  }, [ready]);
  const limit = ready?.limit ?? PAGE_LIMIT;
  const base = `/${owner}/${repoName}/forks`;
  const activeQ = ready?.q ?? "";
  const activeSort = ready?.sort ?? "stars";
  const emptyTitle = activeQ ? "No matching forks" : "No forks yet";
  const emptyDesc = activeQ
    ? "Try a different owner or repository name."
    : "Forks of this repository will appear here.";
  const desc = `${String(total)} forks of ${owner}/${repoName}`;

  async function loadMore() {
    if (loadingMore || forks.length >= total) return;
    const nextOffset = offset + limit;
    setLoadingMore(true);
    try {
      const list = await apiClient.repo.forksList({
        owner,
        name: repoName,
        q: activeQ || null,
        sort: activeSort,
        offset: nextOffset,
        limit,
      });
      if (!list.ok) {
        setLoadingMore(false);
        return;
      }
      setForks([...forks, ...list.data.forks]);
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
      <div data-testid="repo-forks">
        <PageHeader
          icon={<GitFork aria-hidden size={22} />}
          title="Forks"
          description={desc as string}
          actions={
            <div className="flex w-full flex-col gap-2 sm:w-auto sm:flex-row sm:items-center">
              <label className="flex items-center gap-2 text-[13px] text-muted-foreground">
                <span className="sr-only">Sort</span>
                <select
                  className="h-11 rounded-md border border-input bg-card px-3 text-[14px] text-foreground"
                  aria-label="Sort forks"
                  value={sort as string}
                  suppressNativeChangeWarning
                  onChange={(e) => {
                    const next = parseForksSort((e.currentTarget as HTMLSelectElement).value);
                    setSort(next);
                    void navigate(socialListHref(base, { q, sort: next }));
                  }}
                >
                  <option value="stars">{forksSortLabel("stars") as string}</option>
                  <option value="updated">{forksSortLabel("updated") as string}</option>
                  <option value="created">{forksSortLabel("created") as string}</option>
                </select>
              </label>
              <form
                method="get"
                action={base as string}
                className="flex flex-1 gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  void navigate(socialListHref(base, { q, sort }));
                }}
              >
                <input type="hidden" name="sort" value={sort as string} />
                <div className="relative flex-1 sm:w-64">
                  <Search
                    aria-hidden
                    size={16}
                    className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
                  />
                  <Input
                    name="q"
                    value={q as string}
                    onInput={(e) => setQ((e.currentTarget as HTMLInputElement).value)}
                    placeholder="Find a fork"
                    className="pl-8"
                    aria-label="Find a fork"
                    autoComplete="off"
                  />
                </div>
              </form>
            </div>
          }
        />
        @if (forks.length === 0) {
          <EmptyState
            className="mt-8"
            icon={<GitFork aria-hidden size={22} />}
            title={emptyTitle as string}
            description={emptyDesc as string}
          />;
        } @else {
          <List className="mt-6" aria-label="Forks">
            @for (const f of forks; key f.id) {
              <ListRow className="flex flex-col gap-2 py-4 sm:flex-row sm:items-start sm:justify-between">
                <div className="flex min-w-0 items-start gap-3">
                  <AppLink
                    href={("/" + f.owner_username) as string}
                    className="inline-flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-full border border-border bg-muted text-[12px] font-semibold text-foreground no-underline"
                  >
                    @if (f.owner_avatar_url) {
                      <img
                        src={f.owner_avatar_url as string}
                        alt=""
                        className="h-full w-full object-cover"
                      />;
                    } @else {
                      <span>
                        {f.owner_username.slice(0, 1).toUpperCase() as string}
                      </span>;
                    }
                  </AppLink>
                  <div className="min-w-0">
                    <h2 className="truncate text-[15px] font-semibold text-foreground">
                      <AppLink
                        href={("/" + f.owner_username) as string}
                        className="text-foreground no-underline hover:underline"
                      >
                        {f.owner_username as string}
                      </AppLink>
                      <span className="text-muted-foreground"> / </span>
                      <AppLink
                        href={("/" + f.owner_username + "/" + f.name) as string}
                        className="text-foreground no-underline hover:underline"
                      >
                        {f.name as string}
                      </AppLink>
                    </h2>
                    @if (f.description) {
                      <p className="mt-1 line-clamp-2 text-[13px] text-muted-foreground">
                        {f.description as string}
                      </p>;
                    }
                  </div>
                </div>
                <div className="flex flex-wrap items-center gap-3 text-[13px] text-muted-foreground sm:justify-end">
                  <span className="inline-flex items-center gap-1">
                    <Star aria-hidden size={14} />
                    {String(f.star_count)}
                  </span>
                  <span className="inline-flex items-center gap-1">
                    <GitFork aria-hidden size={14} />
                    {String(f.fork_count)}
                  </span>
                  <span>{formatRelativeUpdated(f.updated_at) as string}</span>
                </div>
              </ListRow>;
            }
          </List>;
        }
        @if (forks.length > 0 && forks.length < total) {
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
export function RepoForksPage() @{
  <AppPage>
    <RepoLayout>
      <RepoForksPageContent />
    </RepoLayout>
  </AppPage>;
}
