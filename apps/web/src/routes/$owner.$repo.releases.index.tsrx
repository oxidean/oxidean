import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useQuery } from "@octanejs/tanstack-query";
import type { ReleasePublic, RepoPublic } from "@oxidean/api-client";
import { Rocket, Tag } from "@octanejs/lucide";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";
import { fetchReleaseList, fetchRepoGet } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type ReleasesLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "ready"; repo: RepoPublic; releases: ReleasePublic[] };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadReleasesPage(owner: string, repoName: string): Promise<ReleasesLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const listRes = await fetchReleaseList({ data: { owner, name: repoName } });
  if (!listRes.ok) {
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: listRes.error.message || "Could not load releases.",
    };
  }
  return {
    kind: "ready",
    repo: got.data,
    releases: listRes.data.releases,
  };
}

export const Route = createFileRoute("/$owner/$repo/releases/")({
  component: RepoReleasesPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Releases · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<ReleasesLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    try {
      return await loadReleasesPage(owner, repoName);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Releases list — notes-only (assets in 15-02). */
export function RepoReleasesPage() @{
  const params = useParams({ strict: false }) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const routeLoader = useLoaderData({ from: "/$owner/$repo/releases/" }) as
    | ReleasesLoaderData
    | undefined;

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

  const listQ = useQuery({
    queryKey: ["release", "list", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.release.list({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.releases;
    },
    enabled: !!owner && !!repoName,
    initialData: routeLoader?.kind === "ready" ? routeLoader.releases : undefined,
  });

  const repo = repoQ.data ?? null;
  const notFound =
    layout?.status === "not_found" ||
    routeLoader?.kind === "not_found" ||
    (repoQ.isError && String(repoQ.error?.message ?? "").includes("not found"));
  const newHref = `/${owner}/${repoName}/releases/new`;
  const releases = listQ.data ?? [];

  @if (notFound || (!repoQ.isPending && !repo)) {
    <RepoNotFound />;
  } @else if (!repo) {
    <>
      <PageHeader icon={<Rocket aria-hidden size={22} />} title="Releases" />
      <List className="mt-6">
        @for (const i of [0, 1, 2]; key i) {
          <ListRow className="items-start gap-3">
            <Skeleton className="mt-0.5 size-4 rounded" />
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-4 w-56" />
              <Skeleton className="h-3 w-32" />
            </div>
          </ListRow>;
        }
      </List>
    </>;
  } @else {
    <>
      <PageHeader
        icon={<Rocket aria-hidden size={22} />}
        title="Releases"
        actions={
          repo.can_write && !repo.archived ? (
            <AppLink href={newHref as string} className="no-underline">
              <Button type="button">Draft a new release</Button>
            </AppLink>
          ) : undefined
        }
      />
      @if (listQ.isError) {
        <p role="alert" className="mt-6 text-[16px] text-destructive">
          {(listQ.error?.message || NETWORK_ERROR) as string}
        </p>;
      } @else if (releases.length === 0) {
        <EmptyState
          className="mt-6"
          icon={<Rocket aria-hidden size={22} />}
          title="No releases yet"
          description="Package a point in your history as a downloadable release with notes."
        />;
      } @else {
        <List className="mt-6">
          @for (const rel of releases; key rel.id) {
            <ListRow className="items-start gap-3 py-4">
              <Tag
                aria-hidden
                size={16}
                className="mt-1 shrink-0 text-muted-foreground"
              />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <AppLink
                    href={
                      `/${owner}/${repoName}/releases/${encodeURIComponent(rel.tag_name)}` as string
                    }
                    className="text-[16px] font-semibold text-primary no-underline hover:underline"
                  >
                    {(rel.title.trim() || rel.tag_name) as string}
                  </AppLink>
                  @if (rel.draft) {
                    <Badge variant="draft">Draft</Badge>;
                  }
                  @if (rel.prerelease) {
                    <Badge variant="secondary">Pre-release</Badge>;
                  }
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-2 text-[13px] text-muted-foreground">
                  <span className="font-mono">{rel.tag_name as string}</span>
                  @if (rel.author_username) {
                    <span>{("· by " + rel.author_username) as string}</span>;
                  }
                  @if (rel.created_at) {
                    <span>
                      {("· " + formatRelative(rel.created_at)) as string}
                    </span>;
                  }
                </div>
              </div>
            </ListRow>;
          }
        </List>;
      }
    </>;
  }
}
