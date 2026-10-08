import type { RepoCommitSummary, RepoPublic } from "@oxidean/api-client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useEffect, useState } from "octane";
import { GitCommitHorizontal } from "@octanejs/lucide";
import { CommitAuthorLink } from "@/components/repo/commit-author-link";
import { CommitVerifiedBadge } from "@/components/repo/commit-verified-badge";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { commitHref } from "@/lib/repo-browse";
import { formatRelativeUpdated } from "@/lib/relative-time";
import { fetchRepoCommits, fetchRepoGet } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type CommitsLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      commits: RepoCommitSummary[];
      skip: number;
      limit: number;
    };

type Phase =
  | { kind: "loading" }
  | CommitsLoaderData
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      commits: RepoCommitSummary[];
      skip: number;
      limit: number;
      loadingMore: boolean;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function toReadyPhase(
  data: Extract<CommitsLoaderData, { kind: "ready" }>,
): Extract<Phase, { kind: "ready" }> {
  return { ...data, loadingMore: false };
}

async function loadCommitsPage(
  owner: string,
  repoName: string,
  refFromUrl: string,
): Promise<CommitsLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const repo = got.data;
  const refName = refFromUrl || repo.default_branch || "main";
  const list = await fetchRepoCommits({ owner, name: repoName, ref: refName, skip: 0, limit: 30 });
  if (!list.ok) {
    if (list.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: list.error.message || "Could not load commits.",
    };
  }
  return {
    kind: "ready",
    repo,
    refName: list.data.ref || refName,
    commits: list.data.commits,
    skip: list.data.skip,
    limit: list.data.limit,
  };
}

function repoCommitsQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/commits/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<CommitsLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = String((params as { _splat?: string })._splat ?? "");
      const refFromUrl = decodeURIComponent(splat.split("/")[0] ?? "");
      try {
        return await loadCommitsPage(owner, repoName, refFromUrl);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function relativeCommitTime(iso: string): string {
  return formatRelativeUpdated(iso).replace(/^Updated\s+/i, "");
}

function RepoCommitsPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/commits/$", pathname) ?? {}) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const refFromUrl = decodeURIComponent((params._splat ?? "").split("/")[0] ?? "");
  const loaderData = useQuery(repoCommitsQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() =>
    loaderData
      ? loaderData.kind === "ready"
        ? toReadyPhase(loaderData)
        : loaderData
      : { kind: "loading" },
  );

  useEffect(() => {
    if (loaderData) {
      setPhase(loaderData.kind === "ready" ? toReadyPhase(loaderData) : loaderData);
      return;
    }
    let cancelled = false;
    void (async () => {
      setPhase({ kind: "loading" });
      try {
        const next = await loadCommitsPage(owner, repoName, refFromUrl);
        if (cancelled) return;
        setPhase(next.kind === "ready" ? toReadyPhase(next) : next);
      } catch {
        if (!cancelled) setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, refFromUrl, loaderData]);

  async function loadMore() {
    if (phase.kind !== "ready" || phase.loadingMore) return;
    const nextSkip = phase.skip + phase.limit;
    setPhase({ ...phase, loadingMore: true });
    try {
      const list = await apiClient.repo.commits({
        owner,
        name: repoName,
        ref: phase.refName,
        skip: nextSkip,
        limit: phase.limit,
      });
      if (!list.ok) {
        setPhase({ ...phase, loadingMore: false });
        return;
      }
      setPhase({
        ...phase,
        commits: [...phase.commits, ...list.data.commits],
        skip: list.data.skip,
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
      <PageHeader icon={<GitCommitHorizontal aria-hidden size={22} />} title="Commits" />
      <List className="mt-6">
        @for (const i of [0, 1, 2, 3, 4]; key i) {
          <ListRow className="gap-4">
            <Skeleton className="h-4 w-16" />
            <Skeleton className="h-4 flex-1" />
            <Skeleton className="h-4 w-20" />
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
        icon={<GitCommitHorizontal aria-hidden size={22} />}
        title="Commits"
        description={("on " + phase.refName) as string}
      />
      @if (phase.commits.length === 0) {
        <EmptyState
          className="mt-6"
          icon={<GitCommitHorizontal aria-hidden size={22} />}
          title="No commits yet"
          description="This ref has no commit history to show."
        />;
      } @else {
        <>
          <List className="mt-6">
            @for (const c of phase.commits; key c.sha) {
              <ListRow className="flex-wrap items-baseline gap-x-4 gap-y-1">
                <AppLink
                  href={commitHref(owner, repoName, c.sha) as string}
                  className="font-mono text-[13px] text-primary no-underline hover:underline"
                  title={c.sha as string}
                >
                  {c.short_sha as string}
                </AppLink>
                <AppLink
                  href={commitHref(owner, repoName, c.sha) as string}
                  className="min-w-0 flex-1 truncate text-[15px] font-medium text-foreground no-underline hover:underline"
                  title={c.subject as string}
                >
                  {c.subject as string}
                </AppLink>
                <CommitVerifiedBadge
                  status={c.signature_status}
                  kind={c.signature_kind}
                />
                <CommitAuthorLink
                  name={c.author_name as string}
                  username={c.author_username}
                  avatarUrl={c.author_avatar_url}
                  className="text-[13px]"
                />
                <time
                  className="text-[13px] text-muted-foreground"
                  dateTime={c.authored_at as string}
                >
                  {relativeCommitTime(c.authored_at) as string}
                </time>
              </ListRow>;
            }
          </List>
          @if (phase.commits.length >= phase.limit) {
            <div className="mt-4 flex justify-center">
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={phase.loadingMore}
                onClick={() => {
                  void loadMore();
                }}
              >
                {(phase.loadingMore ? "Loading…" : "Load more") as string}
              </Button>
            </div>;
          }
        </>;
      }
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoCommitsPage() @{
  <AppPage>
    <RepoLayout>
      <RepoCommitsPageContent />
    </RepoLayout>
  </AppPage>;
}
