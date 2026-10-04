import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useQuery } from "@octanejs/tanstack-query";
import { BarChart3, GitFork, Star, Users } from "@octanejs/lucide";
import type {
  RepoForkNetworkNode,
  RepoInsightsCommitActivityResponse,
  RepoInsightsContributorsResponse,
  RepoInsightsForkNetworkResponse,
  RepoPublic,
} from "@oxidean/api-client";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { formatRelative } from "@/lib/relative-time";
import { commitHref } from "@/lib/repo-browse";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";
import {
  fetchRepoGet,
  fetchRepoInsightsCommitActivity,
  fetchRepoInsightsContributors,
  fetchRepoInsightsForkNetwork,
} from "@/lib/ssr-repo";

export type InsightsLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      contributors: RepoInsightsContributorsResponse | null;
      activity: RepoInsightsCommitActivityResponse | null;
      network: RepoInsightsForkNetworkResponse | null;
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const CONTRIBUTOR_LIMIT = 30;
const ACTIVITY_WEEKS = 52;
const NETWORK_LIMIT = 100;

async function loadInsightsPage(owner: string, repoName: string): Promise<InsightsLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return { kind: "error", message: got.error.message || "Could not load repository." };
  }
  // Each section resolves independently — a thrown fetch error must not sink
  // the whole page; failed sections render their own empty/error state.
  const [contributorsRes, activityRes, networkRes] = await Promise.all([
    fetchRepoInsightsContributors({
      data: { owner, name: repoName, limit: CONTRIBUTOR_LIMIT },
    }).catch(() => null),
    fetchRepoInsightsCommitActivity({
      data: { owner, name: repoName, weeks: ACTIVITY_WEEKS },
    }).catch(() => null),
    fetchRepoInsightsForkNetwork({
      data: { owner, name: repoName, limit: NETWORK_LIMIT },
    }).catch(() => null),
  ]);
  return {
    kind: "ready",
    repo: got.data,
    contributors: contributorsRes?.ok ? contributorsRes.data : null,
    activity: activityRes?.ok ? activityRes.data : null,
    network: networkRes?.ok ? networkRes.data : null,
  };
}

type NetworkRow = { node: RepoForkNetworkNode; depth: number };

/**
 * Flatten `nodes` (oldest-first, parent links by owner/name) into an indented
 * tree order: network root first, each fork under its immediate parent. Nodes
 * whose parent is outside the response set render at top level.
 */
function orderNetworkRows(nodes: RepoForkNetworkNode[]): NetworkRow[] {
  const slug = (o: string, n: string) => `${o}/${n}`.toLowerCase();
  const bySlug = new Map<string, RepoForkNetworkNode>();
  for (const n of nodes) bySlug.set(slug(n.owner, n.name), n);
  const childrenOf = new Map<string, RepoForkNetworkNode[]>();
  const roots: RepoForkNetworkNode[] = [];
  for (const n of nodes) {
    const parentSlug = n.parent_owner && n.parent_name ? slug(n.parent_owner, n.parent_name) : null;
    if (parentSlug && bySlug.has(parentSlug)) {
      const list = childrenOf.get(parentSlug) ?? [];
      list.push(n);
      childrenOf.set(parentSlug, list);
    } else {
      roots.push(n);
    }
  }
  roots.sort((a, b) => Number(b.is_root) - Number(a.is_root));
  const out: NetworkRow[] = [];
  const seen = new Set<string>();
  const visit = (n: RepoForkNetworkNode, depth: number) => {
    const key = slug(n.owner, n.name);
    if (seen.has(key)) return;
    seen.add(key);
    out.push({ node: n, depth });
    for (const child of childrenOf.get(key) ?? []) visit(child, depth + 1);
  };
  for (const r of roots) visit(r, 0);
  for (const n of nodes) visit(n, 0); // cycle-safety sweep
  return out;
}

function unixRel(unix: number): string {
  return formatRelative(new Date(unix * 1000).toISOString());
}

function weekLabel(unix: number): string {
  return new Date(unix * 1000).toISOString().slice(0, 10);
}

export const Route = createFileRoute("/$owner/$repo/insights")({
  component: RepoInsightsPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Insights · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<InsightsLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    try {
      return await loadInsightsPage(owner, repoName);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Repository insights — commit activity, contributors, fork network (GIT-26). */
export function RepoInsightsPage() @{
  const params = useParams({ strict: false }) as { owner?: string; repo?: string };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const data = useLoaderData({ from: "/$owner/$repo/insights" }) as InsightsLoaderData | undefined;

  const showNotFound = !data || data.kind === "not_found" || layout?.status === "not_found";
  const errorMessage = data?.kind === "error" ? data.message : "";
  const ready = data?.kind === "ready" ? data : null;

  const activityQ = useQuery({
    queryKey: ["repo", "insights", "commitActivity", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.insightsCommitActivity({
        owner,
        name: repoName,
        weeks: ACTIVITY_WEEKS,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!ready,
    initialData: ready?.activity ?? undefined,
  });
  const contributorsQ = useQuery({
    queryKey: ["repo", "insights", "contributors", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.insightsContributors({
        owner,
        name: repoName,
        limit: CONTRIBUTOR_LIMIT,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!ready,
    initialData: ready?.contributors ?? undefined,
  });
  const networkQ = useQuery({
    queryKey: ["repo", "insights", "forkNetwork", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.insightsForkNetwork({
        owner,
        name: repoName,
        limit: NETWORK_LIMIT,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!ready,
    initialData: ready?.network ?? undefined,
  });

  const activity = activityQ.data ?? null;
  const contributors = contributorsQ.data ?? null;
  const network = networkQ.data ?? null;
  const maxWeekTotal = activity ? Math.max(1, ...activity.weeks.map((w) => w.total)) : 1;
  const networkRows = network ? orderNetworkRows(network.nodes) : [];

  @if (showNotFound) {
    <RepoNotFound />;
  } @else if (errorMessage) {
    <p role="alert" className="text-[16px] text-destructive">
      {errorMessage as string}
    </p>;
  } @else {
    <div data-testid="repo-insights">
      <PageHeader
        icon={<BarChart3 aria-hidden size={22} />}
        title="Insights"
        description={`${owner}/${repoName} · commit activity, contributors, and forks` as string}
      />

      <section className="mt-8" data-testid="repo-insights-commit-activity">
        <h2 className="text-[15px] font-semibold text-foreground">Commit activity</h2>
        @if (activityQ.isError) {
          <p className="mt-3 text-[13px] text-destructive" role="alert">
            {"Could not load commit activity." as string}
          </p>;
        } @else if (!activity) {
          <div className="mt-4 flex h-32 items-end gap-px" aria-hidden="true">
            @for (const i of Array.from(
                { length: ACTIVITY_WEEKS },
                (_, i) => i,
              ); key i) {
              <Skeleton className="h-6 flex-1" />;
            }
          </div>;
        } @else if (activity.total === 0) {
          <EmptyState
            className="mt-4"
            icon={<BarChart3 aria-hidden size={22} />}
            title="No commits in this period"
            description={
              `No commits were pushed to ${ready?.repo.default_branch ?? "the default branch"} in the last ${activity.weeks.length} weeks.` as string
            }
          />;
        } @else {
          <p className="mt-2 text-[13px] text-muted-foreground">
            {
              `${String(activity.total)} commits on ${ready?.repo.default_branch ?? "the default branch"} in the last ${activity.weeks.length} weeks` as string
            }
          </p>;
          <div
            className="mt-4 flex h-32 items-end gap-px"
            role="img"
            aria-label="Weekly commit activity"
            data-testid="repo-insights-activity-chart"
          >
            @for (const w of activity.weeks; key w.week) {
              <div
                className="flex-1 rounded-t-sm bg-primary/70"
                style={{
                  height: `${Math.max(w.total === 0 ? 0 : 4, Math.round((w.total / maxWeekTotal) * 100))}%`,
                }}
                title={`${String(w.total)} commits — week of ${weekLabel(w.week)}`}
              ></div>;
            }
          </div>;
          @if (activity.truncated) {
            <p className="mt-2 text-[12px] text-muted-foreground">
              {
                "History is deeper than the scan window; recent weeks are fully counted." as string
              }
            </p>;
          }
        }
      </section>

      <section className="mt-10" data-testid="repo-insights-contributors">
        <h2 className="text-[15px] font-semibold text-foreground">Contributors</h2>
        @if (contributorsQ.isError) {
          <p className="mt-3 text-[13px] text-destructive" role="alert">
            {"Could not load contributors." as string}
          </p>;
        } @else if (!contributors) {
          <List className="mt-4">
            @for (const i of [0, 1, 2, 3, 4]; key i) {
              <ListRow className="gap-3">
                <Skeleton className="h-8 w-8 rounded-full" />
                <Skeleton className="h-4 flex-1" />
                <Skeleton className="h-4 w-20" />
              </ListRow>;
            }
          </List>;
        } @else if (contributors.contributors.length === 0) {
          <EmptyState
            className="mt-4"
            icon={<Users aria-hidden size={22} />}
            title="No contributors yet"
            description="Contributors appear here once commits land on the default branch."
          />;
        } @else {
          <List className="mt-4" aria-label="Contributors">
            @for (const c of contributors.contributors; key c.email || c.name) {
              <ListRow className="flex items-center gap-3 py-3">
                <span className="inline-flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-full border border-border bg-muted text-[11px] font-semibold text-foreground">
                  @if (c.avatar_url) {
                    <img
                      src={c.avatar_url as string}
                      alt=""
                      className="h-full w-full object-cover"
                    />;
                  } @else {
                    <span>
                      {(c.name || c.email).slice(0, 1).toUpperCase() as string}
                    </span>;
                  }
                </span>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[14px] font-medium text-foreground">
                    @if (c.username) {
                      <a
                        href={("/" + c.username) as string}
                        className="text-foreground no-underline hover:underline"
                      >
                        {c.name as string}
                      </a>;
                      <span className="text-muted-foreground">
                        {(" " + `@${c.username}`) as string}
                      </span>;
                    } @else {
                      <span>{c.name as string}</span>;
                    }
                  </p>
                  <p className="truncate text-[12px] text-muted-foreground">
                    @if (c.last_commit_sha) {
                      <a
                        href={
                          commitHref(owner, repoName, c.last_commit_sha) as string
                        }
                        className="text-muted-foreground no-underline hover:underline"
                      >
                        {("Last commit " + unixRel(c.last_commit_unix)) as string}
                      </a>;
                    } @else {
                      <span>
                        {("Last commit " + unixRel(c.last_commit_unix)) as string}
                      </span>;
                    }
                  </p>
                </div>
                <span className="shrink-0 text-[13px] font-medium text-foreground">
                  {
                    (String(c.commit_count) +
                      (c.commit_count === 1 ? " commit" : " commits")) as string
                  }
                </span>
              </ListRow>;
            }
          </List>;
          @if (contributors.truncated) {
            <p className="mt-2 text-[12px] text-muted-foreground">
              {
                "Showing the top contributors by scanned commits; older history was not walked." as string
              }
            </p>;
          }
        }
      </section>

      <section className="mt-10" data-testid="repo-insights-fork-network">
        <h2 className="text-[15px] font-semibold text-foreground">Fork network</h2>
        @if (networkQ.isError) {
          <p className="mt-3 text-[13px] text-destructive" role="alert">
            {"Could not load the fork network." as string}
          </p>;
        } @else if (!network) {
          <List className="mt-4">
            @for (const i of [0, 1, 2]; key i) {
              <ListRow className="gap-3">
                <Skeleton className="h-4 flex-1" />
                <Skeleton className="h-4 w-16" />
              </ListRow>;
            }
          </List>;
        } @else if (networkRows.length === 0) {
          <EmptyState
            className="mt-4"
            icon={<GitFork aria-hidden size={22} />}
            title="No forks yet"
            description="Forks of this repository will appear here."
          />;
        } @else {
          <List className="mt-4" aria-label="Fork network">
            @for (const row of networkRows; key row.node.id) {
              <ListRow
                className="flex items-center gap-3 py-3"
                style={{ paddingLeft: `${16 + row.depth * 20}px` }}
              >
                <GitFork
                  aria-hidden
                  size={14}
                  className="shrink-0 text-muted-foreground"
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[14px] font-medium text-foreground">
                    <a
                      href={("/" + row.node.owner + "/" + row.node.name) as string}
                      className="text-foreground no-underline hover:underline"
                    >
                      {(row.node.owner + "/" + row.node.name) as string}
                    </a>
                    @if (row.node.is_root) {
                      <span className="ml-2 rounded-md border border-border bg-muted px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground">
                        {"root" as string}
                      </span>;
                    }
                    @if (row.node.is_current) {
                      <span className="ml-2 rounded-md border border-border bg-muted px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground">
                        {"this repository" as string}
                      </span>;
                    }
                  </p>
                  @if (
                    row.node.parent_owner &&
                    row.node.parent_name &&
                    row.depth === 0
                  ) {
                    <p className="truncate text-[12px] text-muted-foreground">
                      {
                        ("forked from " +
                          row.node.parent_owner +
                          "/" +
                          row.node.parent_name) as string
                      }
                    </p>;
                  }
                </div>
                <div className="flex shrink-0 items-center gap-3 text-[12px] text-muted-foreground">
                  <span className="inline-flex items-center gap-1">
                    <Star aria-hidden size={13} />
                    {String(row.node.star_count)}
                  </span>
                  <span className="inline-flex items-center gap-1">
                    <GitFork aria-hidden size={13} />
                    {String(row.node.fork_count)}
                  </span>
                </div>
              </ListRow>;
            }
          </List>;
          @if (network.truncated) {
            <p className="mt-2 text-[12px] text-muted-foreground">
              {
                `Showing ${network.nodes.length} of ${String(network.total)} network members.` as string
              }
            </p>;
          }
        }
      </section>
    </div>;
  }
}
