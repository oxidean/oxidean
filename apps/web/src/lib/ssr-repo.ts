import {
  type RepoSearchHit,
  type RepoSearchResponse,
  type RepoSearchType,
} from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";
import { highlightCode, languageIdForPath, type HighlightTheme } from "@/lib/highlight";
import { clientHighlightTheme } from "@/lib/ssr-auth";
import {
  resolvePublicOriginClient,
  resolveSshAdvertiseHost,
  resolveSshAdvertisePort,
} from "@/lib/public-origin";

type OwnerName = { owner: string; name: string };

/** `repo.get` (private repos included when session owns them). */
export async function fetchRepoGet(data: OwnerName) {
  return apiClient.repo.get({ owner: data.owner, name: data.name });
}

/** `repo.tree`. */
export async function fetchRepoTree(data: OwnerName & { ref: string; path?: string }) {
  return apiClient.repo.tree({
    owner: data.owner,
    name: data.name,
    ref: data.ref,
    path: data.path ?? "",
  });
}

/** `repo.refs`. */
export async function fetchRepoRefs(data: OwnerName) {
  return apiClient.repo.refs({ owner: data.owner, name: data.name });
}

/** `repo.templates.list` — issue/PR file templates from the git tree (COL-02). */
export async function fetchRepoFileTemplates(data: OwnerName) {
  return apiClient.repo.templates.list({ owner: data.owner, name: data.name });
}

/** `repo.blob`. */
export async function fetchRepoBlob(data: OwnerName & { ref: string; path: string }) {
  return apiClient.repo.blob({
    owner: data.owner,
    name: data.name,
    ref: data.ref,
    path: data.path,
  });
}

/** `repo.commits`. */
export async function fetchRepoCommits(
  data: OwnerName & { ref: string; skip?: number; limit?: number },
) {
  return apiClient.repo.commits({
    owner: data.owner,
    name: data.name,
    ref: data.ref,
    skip: typeof data.skip === "number" ? data.skip : 0,
    limit: typeof data.limit === "number" ? data.limit : 30,
  });
}

/** `repo.pathLastCommits` (issue #23). */
export async function fetchRepoPathLastCommits(data: OwnerName & { ref: string; path?: string }) {
  return apiClient.repo.pathLastCommits({
    owner: data.owner,
    name: data.name,
    ref: data.ref,
    path: data.path || undefined,
  });
}

/** `repo.commitCount` (issue #23). */
export async function fetchRepoCommitCount(data: OwnerName & { ref: string }) {
  return apiClient.repo.commitCount({
    owner: data.owner,
    name: data.name,
    ref: data.ref,
  });
}

/** `repo.contributors.list` (issue #23). */
export async function fetchRepoContributors(data: OwnerName & { limit?: number }) {
  return apiClient.repo.contributorsList({
    owner: data.owner,
    name: data.name,
    limit: typeof data.limit === "number" ? data.limit : 30,
  });
}

/** `repo.languages` — About sidebar language bar (linguist-lite). */
export async function fetchRepoLanguages(data: OwnerName) {
  return apiClient.repo.languages({
    owner: data.owner,
    name: data.name,
  });
}

/** `repo.activity.list` — push activity feed. */
export async function fetchRepoActivity(
  data: OwnerName & {
    push_type?: string;
    period?: string;
    offset?: number;
    limit?: number;
  },
) {
  return apiClient.repo.activityList({
    owner: data.owner,
    name: data.name,
    push_type: data.push_type?.trim() || null,
    period: data.period?.trim() || "all",
    offset: Number(data.offset ?? 0),
    limit: Number(data.limit ?? 30),
  });
}

/** `repo.insights.contributors` — Insights tab committer table (GIT-26). */
export async function fetchRepoInsightsContributors(data: OwnerName & { limit?: number }) {
  return apiClient.repo.insightsContributors({
    owner: data.owner,
    name: data.name,
    limit: typeof data.limit === "number" ? data.limit : 30,
  });
}

/** `repo.insights.commitActivity` — weekly buckets (GIT-26). */
export async function fetchRepoInsightsCommitActivity(data: OwnerName & { weeks?: number }) {
  return apiClient.repo.insightsCommitActivity({
    owner: data.owner,
    name: data.name,
    weeks: typeof data.weeks === "number" ? data.weeks : 52,
  });
}

/** `repo.insights.forkNetwork` — fork-network member rows (GIT-26). */
export async function fetchRepoInsightsForkNetwork(data: OwnerName & { limit?: number }) {
  return apiClient.repo.insightsForkNetwork({
    owner: data.owner,
    name: data.name,
    limit: typeof data.limit === "number" ? data.limit : 100,
  });
}

/** `packages.list` filtered by repository_id (issue #23 About). */
export async function fetchPackagesForRepo(data: { repository_id: string }) {
  return apiClient.packages.list({ repository_id: data.repository_id ?? "" });
}

/** `packages.list` by owner and/or repository_id (packages pages). */
export async function fetchPackagesList(data: { owner?: string; repository_id?: string }) {
  return apiClient.packages.list({
    owner: data.owner ? String(data.owner) : null,
    repository_id: data.repository_id ? String(data.repository_id) : null,
  });
}

/** `repo.actions.listRuns` (anonymous reads on public repos). */
export async function fetchActionsListRuns(
  data: OwnerName & {
    page?: number;
    per_page?: number;
    status?: string;
    event?: string;
    branch?: string;
    workflow?: string;
    actor?: string;
    query?: string;
  },
) {
  return apiClient.repo.actions.listRuns({
    owner: data.owner,
    name: data.name,
    page: typeof data.page === "number" ? data.page : 1,
    per_page: typeof data.per_page === "number" ? data.per_page : 25,
    status: data.status ? String(data.status) : undefined,
    event: data.event ? String(data.event) : undefined,
    branch: data.branch ? String(data.branch) : undefined,
    workflow: data.workflow ? String(data.workflow) : undefined,
    actor: data.actor ? String(data.actor) : undefined,
    query: data.query ? String(data.query) : undefined,
  });
}

/** `repo.actions.listWorkflows`. */
export async function fetchActionsListWorkflows(data: OwnerName & { git_ref?: string }) {
  return apiClient.repo.actions.listWorkflows({
    owner: data.owner,
    name: data.name,
    git_ref: data.git_ref ? String(data.git_ref) : undefined,
  });
}

/** `repo.actions.getRun`. */
export async function fetchActionsGetRun(data: OwnerName & { run_id: string }) {
  return apiClient.repo.actions.getRun({
    owner: data.owner,
    name: data.name,
    run_id: String(data.run_id ?? ""),
  });
}

/** `repo.actions.getJobLog`. */
export async function fetchActionsGetJobLog(data: OwnerName & { run_id: string; job_id: string }) {
  return apiClient.repo.actions.getJobLog({
    owner: data.owner,
    name: data.name,
    run_id: String(data.run_id ?? ""),
    job_id: String(data.job_id ?? ""),
  });
}

/** `repo.stargazers.list` (Write+ gated). */
export async function fetchRepoStargazers(
  data: OwnerName & { q?: string; offset?: number; limit?: number },
) {
  return apiClient.repo.stargazersList({
    owner: data.owner,
    name: data.name,
    q: data.q?.trim() || null,
    offset: Number(data.offset ?? 0),
    limit: Number(data.limit ?? 30),
  });
}

/** `repo.watchers.list`. */
export async function fetchRepoWatchers(
  data: OwnerName & { q?: string; offset?: number; limit?: number },
) {
  return apiClient.repo.watchersList({
    owner: data.owner,
    name: data.name,
    q: data.q?.trim() || null,
    offset: Number(data.offset ?? 0),
    limit: Number(data.limit ?? 30),
  });
}

/** `repo.forks.list`. */
export async function fetchRepoForks(
  data: OwnerName & {
    q?: string;
    sort?: string;
    offset?: number;
    limit?: number;
  },
) {
  return apiClient.repo.forksList({
    owner: data.owner,
    name: data.name,
    q: data.q?.trim() || null,
    sort: data.sort ? String(data.sort) : "stars",
    offset: Number(data.offset ?? 0),
    limit: Number(data.limit ?? 30),
  });
}

/** `repo.blame`. */
export async function fetchRepoBlame(data: OwnerName & { ref: string; path: string }) {
  return apiClient.repo.blame({
    owner: data.owner,
    name: data.name,
    ref: data.ref,
    path: data.path,
  });
}

/** `repo.commit`. */
export async function fetchRepoCommit(data: OwnerName & { sha: string }) {
  return apiClient.repo.commit({
    owner: data.owner,
    name: data.name,
    sha: data.sha,
  });
}

/** `repo.compare`. */
export async function fetchRepoCompare(data: OwnerName & { base: string; head: string }) {
  return apiClient.repo.compare({
    owner: data.owner,
    name: data.name,
    base: data.base,
    head: data.head,
  });
}

/** `repo.search` hit carrying highlighted HTML when the client highlighted it. */
export type SsrRepoSearchHit = RepoSearchHit & { html?: string };

export type SsrRepoSearchResult =
  | {
      ok: true;
      data: Omit<RepoSearchResponse, "hits"> & { hits: SsrRepoSearchHit[] };
      highlightTheme: HighlightTheme | null;
    }
  | { ok: false; error: { code?: string; message?: string } };

/**
 * `repo.search` + client-side Shiki for code hits — keeps the "highlighted on
 * first paint" behavior without a server pass.
 */
export async function fetchRepoSearch(
  data: OwnerName & { type?: string; q?: string },
): Promise<SsrRepoSearchResult> {
  const res = await apiClient.repo.search({
    owner: data.owner,
    name: data.name,
    type: (data.type ? String(data.type) : "code") as RepoSearchType,
    q: String(data.q ?? ""),
  });
  if (!res.ok) {
    return { ok: false, error: res.error };
  }
  try {
    const theme = clientHighlightTheme();
    const hits = await Promise.all(
      res.data.hits.map(async (hit): Promise<SsrRepoSearchHit> => {
        if (hit.kind !== "code" || !hit.content) return { ...hit };
        try {
          const html = await highlightCode(hit.content, {
            lang: languageIdForPath(hit.path),
            theme,
          });
          return { ...hit, html };
        } catch {
          return { ...hit };
        }
      }),
    );
    return { ok: true, data: { ...res.data, hits }, highlightTheme: theme };
  } catch {
    return { ok: true, data: { ...res.data }, highlightTheme: null };
  }
}

/** `issue.list`. */
export async function fetchIssueList(
  data: OwnerName & {
    state?: string;
    author?: string;
    label?: string;
    assignee?: string;
    q?: string;
    offset?: number;
    limit?: number;
  },
) {
  return apiClient.issue.list({
    owner: data.owner,
    name: data.name,
    state: data.state ? String(data.state) : "open",
    author: data.author ? String(data.author) : null,
    label: data.label ? String(data.label) : null,
    assignee: data.assignee ? String(data.assignee) : null,
    q: data.q ? String(data.q) : null,
    offset: typeof data.offset === "number" ? data.offset : 0,
    limit: typeof data.limit === "number" ? data.limit : 25,
  });
}

/** `label.listForRepo`. */
export async function fetchLabelListForRepo(data: OwnerName) {
  return apiClient.label.listForRepo({ owner: data.owner, name: data.name });
}

/** `issue.get`. */
export async function fetchIssueGet(data: OwnerName & { number: number }) {
  return apiClient.issue.get({
    owner: data.owner,
    name: data.name,
    number: typeof data.number === "number" ? data.number : Number(data.number),
  });
}

/** `pull.get`. */
export async function fetchPullGet(data: OwnerName & { number: number }) {
  return apiClient.pull.get({
    owner: data.owner,
    name: data.name,
    number: typeof data.number === "number" ? data.number : Number(data.number),
  });
}

/** `pull.list`. */
export async function fetchPullList(
  data: OwnerName & { state?: string | null; offset?: number; limit?: number },
) {
  return apiClient.pull.list({
    owner: data.owner,
    name: data.name,
    state: data.state == null || data.state === "" ? null : String(data.state),
    offset: typeof data.offset === "number" ? data.offset : Number(data.offset ?? 0),
    limit: typeof data.limit === "number" ? data.limit : Number(data.limit ?? 25),
  });
}

/** `release.list`. */
export async function fetchReleaseList(data: OwnerName) {
  return apiClient.release.list({ owner: data.owner, name: data.name });
}

/**
 * Browser-facing origin for clone URLs — the page's own origin under the
 * static-serving model.
 */
export async function fetchPublicOrigin() {
  return resolvePublicOriginClient();
}

/**
 * Advertised Git SSH host + port for CloneBox — read from
 * `oxidean:ssh-host`/`oxidean:ssh-port` meta injected by the serving
 * middleware (the only tier that can see OXIDEAN_SSH_* env).
 */
export async function fetchSshAdvertise(data: { publicOrigin?: string } = {}) {
  const publicOrigin = (data.publicOrigin ?? "").trim() || resolvePublicOriginClient();
  return {
    sshHost: resolveSshAdvertiseHost(publicOrigin),
    sshPort: resolveSshAdvertisePort(),
  };
}
