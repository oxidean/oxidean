import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { Search, Star } from "@octanejs/lucide";
import type { RepoPublic, RepoStargazerPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { EmptyState } from "@/components/ui/empty-state";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { apiClient } from "@/lib/api-client";
import { useAppNavigate } from "@/lib/app-navigate";
import { socialListHref } from "@/lib/repo-social-lists";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";
import { fetchRepoGet, fetchRepoStargazers } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type StargazersLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      stargazers: RepoStargazerPublic[];
      total: number;
      q: string;
      offset: number;
      limit: number;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const PAGE_LIMIT = 50;

async function loadStargazersPage(
  owner: string,
  repoName: string,
  q: string,
): Promise<StargazersLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return { kind: "error", message: got.error.message || "Could not load repository." };
  }
  if (!got.data.can_write) return { kind: "not_found" };

  const listRes = await fetchRepoStargazers({
    data: { owner, name: repoName, q, offset: 0, limit: PAGE_LIMIT },
  });
  if (!listRes.ok) {
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: listRes.error.message || "Could not load stargazers.",
    };
  }
  return {
    kind: "ready",
    repo: got.data,
    stargazers: listRes.data.stargazers,
    total: listRes.data.total,
    q,
    offset: 0,
    limit: PAGE_LIMIT,
  };
}

export const Route = createFileRoute("/$owner/$repo/stargazers")({
  component: RepoStargazersPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Stargazers · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  // Loader consumes ?q — loaderDeps keeps the match id search-aware so a
  // search-only navigation refetches instead of reusing stale loaderData.
  loaderDeps: ({ search }) => search,
  loader: async ({ params, location }): Promise<StargazersLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    const q = new URLSearchParams(location.searchStr.replace(/^\?/, "")).get("q") ?? "";
    try {
      return await loadStargazersPage(owner, repoName, q);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Stargazers list — Write+ only (About sidebar + soft 404 for others). */
export function RepoStargazersPage() @{
  const params = useParams({ strict: false }) as { owner?: string; repo?: string };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const data = useLoaderData({ from: "/$owner/$repo/stargazers" }) as
    | StargazersLoaderData
    | undefined;

  const showNotFound = !data || data.kind === "not_found" || layout?.status === "not_found";
  const errorMessage = data?.kind === "error" ? data.message : "";
  const ready = data?.kind === "ready" ? data : null;
  const [q, setQ] = useState(ready?.q ?? "");
  const navigateApp = useAppNavigate();
  const [stargazers, setStargazers] = useState(ready?.stargazers ?? []);
  const [offset, setOffset] = useState(ready?.offset ?? 0);
  const [total, setTotal] = useState(ready?.total ?? 0);
  const [loadingMore, setLoadingMore] = useState(false);
  const limit = ready?.limit ?? PAGE_LIMIT;
  const base = `/${owner}/${repoName}/stargazers`;
  const activeQ = ready?.q ?? "";
  const emptyTitle = activeQ ? "No matching stargazers" : "There are no stargazers yet";
  const emptyDesc = activeQ
    ? "Try a different username or clear the search."
    : "People who star this repository will appear here.";
  const desc = `${String(total)} starring ${owner}/${repoName}`;

  async function loadMore() {
    if (loadingMore || stargazers.length >= total) return;
    const nextOffset = offset + limit;
    setLoadingMore(true);
    try {
      const list = await apiClient.repo.stargazersList({
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
      setStargazers([...stargazers, ...list.data.stargazers]);
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
      <div data-testid="repo-stargazers">
        <PageHeader
          icon={<Star aria-hidden size={22} />}
          title="Stargazers"
          description={desc as string}
          actions={
            <form
              method="get"
              action={base as string}
              className="flex w-full gap-2 sm:w-auto"
              onSubmit={(e) => {
                e.preventDefault();
                navigateApp(socialListHref(base, { q }));
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
                  placeholder="Find a stargazer"
                  className="pl-8"
                  aria-label="Find a stargazer"
                  autoComplete="off"
                />
              </div>
            </form>
          }
        />
        @if (stargazers.length === 0) {
          <EmptyState
            className="mt-8"
            icon={<Star aria-hidden size={22} />}
            title={emptyTitle as string}
            description={emptyDesc as string}
          />;
        } @else {
          <List className="mt-6" aria-label="Stargazers">
            @for (const s of stargazers; key s.user_id) {
              <ListRow className="flex items-center gap-3 py-3">
                <AppLink
                  href={("/" + s.username) as string}
                  className="inline-flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-full border border-border bg-muted text-[12px] font-semibold text-foreground no-underline"
                >
                  @if (s.avatar_url) {
                    <img
                      src={s.avatar_url as string}
                      alt=""
                      className="h-full w-full object-cover"
                    />;
                  } @else {
                    <span>
                      {s.display_name.slice(0, 1).toUpperCase() as string}
                    </span>;
                  }
                </AppLink>
                <div className="min-w-0 flex-1">
                  <AppLink
                    href={("/" + s.username) as string}
                    className="font-medium text-foreground no-underline hover:underline"
                  >
                    {s.username as string}
                  </AppLink>
                  @if (
                    s.display_name &&
                    s.display_name !== s.username
                  ) {
                    <span className="ml-1.5 text-[13px] text-muted-foreground">
                      {s.display_name as string}
                    </span>;
                  }
                </div>
              </ListRow>;
            }
          </List>;
        }
        @if (stargazers.length > 0 && stargazers.length < total) {
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
