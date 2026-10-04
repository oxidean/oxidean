import { createFileRoute, Link, useLoaderData } from "@octanejs/tanstack-router";
import { createServerFn } from "@octanejs/tanstack-start";
import { getRequestHeader } from "@octanejs/tanstack-start/server";
import { useState } from "octane";
import { BookMarked, Compass, Search, Star } from "@octanejs/lucide";
import { createClient, type RepoPublic } from "@oxidean/api-client";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { AppLink } from "@/components/ui/app-link";

function ssrApiOrigin(): string {
  return (
    process.env.OXIDEAN_API_ORIGIN?.replace(/\/$/, "") ||
    process.env.OXIDEAN_E2E_API_ORIGIN?.replace(/\/$/, "") ||
    "http://127.0.0.1:8080"
  );
}

const fetchExplore = createServerFn({ method: "GET" })
  .validator((data: { q?: string }) => ({
    q: data?.q ? String(data.q) : "",
  }))
  .handler(async ({ data }): Promise<{ repos: RepoPublic[]; q: string }> => {
    const cookie = getRequestHeader("cookie") ?? "";
    const client = createClient({
      baseUrl: ssrApiOrigin(),
      credentials: "include",
      fetch: (input, init) => {
        const headers = new Headers(init?.headers);
        if (cookie) headers.set("cookie", cookie);
        return fetch(input, { ...init, headers });
      },
    });
    const q = data.q.trim();
    const res = await client.repo.explore({
      q: q || null,
      offset: 0,
      limit: 40,
    });
    return { repos: res.ok ? res.data.repos : [], q };
  });

export type ExploreLoaderData = { repos: RepoPublic[]; q: string };

export const Route = createFileRoute("/explore")({
  component: ExplorePage,
  head: () => ({ meta: [{ title: "Explore · Oxidean" }] }),
  // Loader consumes ?q — loaderDeps keeps the match id search-aware so a
  // search-only navigation refetches instead of reusing stale loaderData.
  loaderDeps: ({ search }) => search,
  loader: async ({ location }): Promise<ExploreLoaderData> => {
    const q = new URLSearchParams(location.searchStr.replace(/^\?/, "")).get("q") ?? "";
    return fetchExplore({ data: { q } });
  },
});

export function ExplorePage() @{
  const data = useLoaderData({ from: "/explore" }) as ExploreLoaderData | undefined;
  const repos = data?.repos ?? [];
  const [q, setQ] = useState(data?.q ?? "");

  <div className="relative overflow-hidden">
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-x-0 top-0 h-64 opacity-40"
      style={{
        background:
          "radial-gradient(ellipse 80% 60% at 20% 0%, color-mix(in oklab, var(--primary) 18%, transparent), transparent), radial-gradient(ellipse 60% 50% at 90% 0%, color-mix(in oklab, var(--chart-2, var(--secondary)) 12%, transparent), transparent)",
      }}
    />
    <div className="relative mx-auto max-w-6xl px-4 py-10">
      <PageHeader
        icon={<Compass aria-hidden size={22} />}
        title="Explore"
        description="Discover public repositories on this instance, sorted by stars."
        actions={
          <form
            method="get"
            action="/explore"
            className="flex w-full gap-2 sm:w-auto"
            onSubmit={(e) => {
              e.preventDefault();
              const next = q.trim();
              window.location.assign(next ? `/explore?q=${encodeURIComponent(next)}` : "/explore");
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
                placeholder="Filter repositories"
                className="pl-8"
                aria-label="Search explore"
              />
            </div>
            <Button type="submit">Search</Button>
          </form>
        }
      />

      @if (repos.length === 0) {
        <EmptyState
          className="mt-10"
          icon={<BookMarked aria-hidden size={22} />}
          title={q.trim() ? "No matching repositories" : "No public repositories yet"}
          description={
            q.trim()
              ? "Try a different search term, or clear the filter to see everything."
              : "Public repositories on this instance will appear here once they exist."
          }
          action={
            q.trim() ? (
              <AppLink
                href="/explore"
                className="text-[14px] font-medium text-primary hover:underline"
              >
                Clear filter
              </AppLink>
            ) : undefined
          }
        />;
      } @else {
        <List className="mt-8">
          @for (const repo of repos; key repo.id) {
            <ListRow className="items-start gap-3 py-4">
              <BookMarked
                aria-hidden
                size={18}
                className="mt-0.5 shrink-0 text-muted-foreground"
              />
              <div className="min-w-0 flex-1">
                <Link
                  to="/$owner/$repo"
                  params={{ owner: repo.owner_username, repo: repo.name }}
                  preload="intent"
                  className="font-[family-name:var(--font-display)] text-[16px] font-semibold text-primary no-underline hover:underline"
                >
                  {(repo.owner_username + "/" + repo.name) as string}
                </Link>
                @if (repo.description?.trim()) {
                  <p className="mt-1 line-clamp-2 text-[14px] text-muted-foreground">
                    {repo.description as string}
                  </p>;
                }
                <div className="mt-2 flex items-center gap-1.5 text-[12px] text-muted-foreground">
                  <Star aria-hidden size={13} />
                  <span>{String(repo.star_count ?? 0)}</span>
                </div>
              </div>
            </ListRow>;
          }
        </List>;
      }
    </div>
  </div>;
}
