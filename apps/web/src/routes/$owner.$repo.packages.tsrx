import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import {
  packagesListQueryOptions,
  type PackagesListResponse,
  type RepoPublic,
} from "@oxidean/api-client";
import { Package } from "@octanejs/lucide";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { useRepoStore } from "@/lib/repo-store";
import { fetchPackagesList, fetchRepoGet } from "@/lib/ssr-repo";
import { PublishQuickstart } from "@/components/packages/publish-quickstart";
import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { Skeleton } from "@/components/ui/skeleton";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

export type RepoPackagesLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      list: PackagesListResponse;
    };

async function loadRepoPackagesPage(
  owner: string,
  repoName: string,
): Promise<RepoPackagesLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const list = await fetchPackagesList({ repository_id: got.data.id });
  if (!list.ok) {
    return {
      kind: "error",
      message: list.error.message || "Could not load packages.",
    };
  }
  return { kind: "ready", repo: got.data, list: list.data };
}

function repoPackagesQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/packages",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<RepoPackagesLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      try {
        return await loadRepoPackagesPage(owner, repoName);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/**
 * Repo-linked packages (D-PKG-11) — scoped by repository_id.
 * Chrome/shell come from `$owner.$repo` layout (D-QH-01).
 */
function RepoPackagesPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/packages", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const owner = String(params.owner ?? "");
  const repoName = String(params.repo ?? "");
  const routeLoader = useQuery(repoPackagesQueryOptions(params, searchStr)).data;

  const layoutRepo = useRepoStore((s) => s.repo);
  const repositoryId = layoutRepo?.id ?? null;

  // Fallback repo.get when rendered outside layout (tests / edge); still scopes by repository_id.
  const repoQ = useQuery({
    queryKey: ["repo", "get", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.get({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !repositoryId && !!owner && !!repoName,
    initialData: routeLoader?.kind === "ready" ? routeLoader.repo : undefined,
  });

  const scopedRepositoryId = repositoryId ?? repoQ.data?.id ?? null;
  const ssrListMatches =
    routeLoader?.kind === "ready" && scopedRepositoryId === routeLoader.repo.id;
  const listQ = useQuery({
    ...packagesListQueryOptions(apiClient, {
      repository_id: scopedRepositoryId,
    }),
    enabled: !!scopedRepositoryId,
    initialData: ssrListMatches ? routeLoader.list : undefined,
  });

  const packages = listQ.data?.packages ?? (ssrListMatches ? routeLoader.list.packages : []);
  const loadError =
    routeLoader?.kind === "error"
      ? routeLoader.message
      : listQ.isError
        ? String(listQ.error?.message || NETWORK_ERROR)
        : "";
  const loading =
    !loadError && ((!repositoryId && repoQ.isPending) || (!!scopedRepositoryId && listQ.isPending));

  @{
    <div data-testid="repo-packages">
      <h2 className="font-[family-name:var(--font-display)] text-[20px] font-semibold text-foreground">
        Packages
      </h2>
      <p className="mt-1 text-muted-foreground">Packages linked to this repository.</p>
      @if (loadError) {
        <p className="mt-6 text-destructive" role="alert">
          {loadError as string}
        </p>;
      } @else if (loading) {
        <List className="mt-6">
          @for (const i of [0, 1, 2]; key i) {
            <ListRow>
              <Skeleton className="size-4 rounded-full" />
              <div className="flex flex-1 flex-col gap-2">
                <Skeleton className="h-4 w-48" />
                <Skeleton className="h-3 w-24" />
              </div>
            </ListRow>;
          }
        </List>;
      } @else {
        @if (packages.length === 0) {
          <>
            <EmptyState
              className="mt-6"
              icon={<Package aria-hidden size={22} />}
              title="No packages published yet"
              description="Publish a package with your favorite package client, then link it to this repository."
            />
            <PublishQuickstart owner={owner} className="mt-6" />
          </>;
        } @else {
          <List className="mt-6">
            @for (const pkg of packages; key pkg.id) {
              <ListRow className="items-center gap-3 py-3">
                <Package
                  aria-hidden
                  size={16}
                  className="shrink-0 text-muted-foreground"
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-[15px] font-semibold text-foreground">
                      {pkg.name as string}
                    </span>
                    <Badge variant="outline" className="uppercase">
                      {pkg.format as string}
                    </Badge>
                  </div>
                  <p className="mt-0.5 text-[12px] text-muted-foreground">
                    {
                      `${pkg.versions.length} version${pkg.versions.length === 1 ? "" : "s"}${pkg.versions[0] ? ` · updated ${formatRelative(pkg.versions[0].created_at)}` : ""}` as string
                    }
                  </p>
                </div>
              </ListRow>;
            }
          </List>;
        }
      }
    </div>;
    };
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoPackagesPage() @{
  <AppPage>
    <RepoLayout>
      <RepoPackagesPageContent />
    </RepoLayout>
  </AppPage>;
}
