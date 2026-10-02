import { createServerFn } from "@octanejs/tanstack-start";
import { getRequestHeader } from "@octanejs/tanstack-start/server";
import { createClient, type OxideanClient } from "@oxidean/api-client";
import { resolvePublicOriginFromEnv, resolveSshHost, resolveSshPort } from "@/lib/public-origin";

/** API origin for SSR Cookie-forward RPCs — never the browser origin during SSR. */
function ssrApiOrigin(): string {
  return (
    process.env.OXIDEAN_API_ORIGIN?.replace(/\/$/, "") ||
    process.env.OXIDEAN_E2E_API_ORIGIN?.replace(/\/$/, "") ||
    "http://127.0.0.1:8080"
  );
}

/**
 * Cookie-forward Oxidean RPC client for repo SSR loaders.
 * Forwards the incoming request Cookie only — never logs cookie values (T-06-11).
 */
function createSsrClient(cookie: string): OxideanClient {
  return createClient({
    baseUrl: ssrApiOrigin(),
    credentials: "include",
    fetch: (input, init) => {
      const headers = new Headers(init?.headers);
      if (cookie) {
        headers.set("cookie", cookie);
      }
      return fetch(input, { ...init, headers });
    },
  });
}

function incomingCookie(): string {
  return getRequestHeader("cookie") ?? "";
}

type OwnerName = { owner: string; name: string };

function ownerNameValidator(data: OwnerName): OwnerName {
  return {
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
  };
}

/** SSR: `repo.get` with Cookie forward (private repos included when session owns them). */
export const fetchRepoGet = createServerFn({ method: "GET" })
  .validator(ownerNameValidator)
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.get({ owner: data.owner, name: data.name });
  });

/** SSR: `repo.tree` with Cookie forward. */
export const fetchRepoTree = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { ref: string; path?: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    ref: String(data?.ref ?? ""),
    path: String(data?.path ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.tree({
      owner: data.owner,
      name: data.name,
      ref: data.ref,
      path: data.path,
    });
  });

/** SSR: `repo.refs` with Cookie forward. */
export const fetchRepoRefs = createServerFn({ method: "GET" })
  .validator(ownerNameValidator)
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.refs({ owner: data.owner, name: data.name });
  });

/** SSR: `repo.blob` with Cookie forward. */
export const fetchRepoBlob = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { ref: string; path: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    ref: String(data?.ref ?? ""),
    path: String(data?.path ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.blob({
      owner: data.owner,
      name: data.name,
      ref: data.ref,
      path: data.path,
    });
  });

/** SSR: `repo.commits` with Cookie forward. */
export const fetchRepoCommits = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { ref: string; skip?: number; limit?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    ref: String(data?.ref ?? ""),
    skip: typeof data?.skip === "number" ? data.skip : 0,
    limit: typeof data?.limit === "number" ? data.limit : 30,
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.commits({
      owner: data.owner,
      name: data.name,
      ref: data.ref,
      skip: data.skip,
      limit: data.limit,
    });
  });

/** SSR: `repo.pathLastCommits` with Cookie forward (issue #23). */
export const fetchRepoPathLastCommits = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { ref: string; path?: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    ref: String(data?.ref ?? ""),
    path: String(data?.path ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.pathLastCommits({
      owner: data.owner,
      name: data.name,
      ref: data.ref,
      path: data.path || undefined,
    });
  });

/** SSR: `repo.commitCount` with Cookie forward (issue #23). */
export const fetchRepoCommitCount = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { ref: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    ref: String(data?.ref ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.commitCount({
      owner: data.owner,
      name: data.name,
      ref: data.ref,
    });
  });

/** SSR: `repo.contributors.list` with Cookie forward (issue #23). */
export const fetchRepoContributors = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { limit?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    limit: typeof data?.limit === "number" ? data.limit : 30,
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.contributorsList({
      owner: data.owner,
      name: data.name,
      limit: data.limit,
    });
  });

/** SSR: `repo.languages` — About sidebar language bar (linguist-lite). */
export const fetchRepoLanguages = createServerFn({ method: "GET" })
  .validator((data: OwnerName) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.languages({
      owner: data.owner,
      name: data.name,
    });
  });

/** SSR: `repo.activity.list` — push activity feed. */
export const fetchRepoActivity = createServerFn({ method: "GET" })
  .validator(
    (
      data: OwnerName & {
        push_type?: string;
        period?: string;
        offset?: number;
        limit?: number;
      },
    ) => ({
      owner: String(data?.owner ?? ""),
      name: String(data?.name ?? ""),
      push_type: data?.push_type ? String(data.push_type) : "",
      period: data?.period ? String(data.period) : "all",
      offset: Number(data?.offset ?? 0),
      limit: Number(data?.limit ?? 30),
    }),
  )
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.activityList({
      owner: data.owner,
      name: data.name,
      push_type: data.push_type.trim() || null,
      period: data.period.trim() || "all",
      offset: data.offset,
      limit: data.limit,
    });
  });

/** SSR: `repo.insights.contributors` — Insights tab committer table (GIT-26). */
export const fetchRepoInsightsContributors = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { limit?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    limit: typeof data?.limit === "number" ? data.limit : 30,
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.insightsContributors({
      owner: data.owner,
      name: data.name,
      limit: data.limit,
    });
  });

/** SSR: `repo.insights.commitActivity` — weekly buckets (GIT-26). */
export const fetchRepoInsightsCommitActivity = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { weeks?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    weeks: typeof data?.weeks === "number" ? data.weeks : 52,
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.insightsCommitActivity({
      owner: data.owner,
      name: data.name,
      weeks: data.weeks,
    });
  });

/** SSR: `repo.insights.forkNetwork` — fork-network member rows (GIT-26). */
export const fetchRepoInsightsForkNetwork = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { limit?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    limit: typeof data?.limit === "number" ? data.limit : 100,
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.insightsForkNetwork({
      owner: data.owner,
      name: data.name,
      limit: data.limit,
    });
  });

/** SSR: `packages.list` filtered by repository_id (issue #23 About). */
export const fetchPackagesForRepo = createServerFn({ method: "GET" })
  .validator((data: { repository_id: string }) => ({
    repository_id: String(data?.repository_id ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.packages.list({ repository_id: data.repository_id });
  });

/** SSR: `packages.list` by owner and/or repository_id (packages pages). */
export const fetchPackagesList = createServerFn({ method: "GET" })
  .validator((data: { owner?: string; repository_id?: string }) => ({
    owner: data?.owner ? String(data.owner) : null,
    repository_id: data?.repository_id ? String(data.repository_id) : null,
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.packages.list({
      owner: data.owner,
      repository_id: data.repository_id,
    });
  });

/** SSR: `repo.actions.listRuns` with Cookie forward (anonymous reads on public repos). */
export const fetchActionsListRuns = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { page?: number; per_page?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    page: typeof data?.page === "number" ? data.page : 1,
    per_page: typeof data?.per_page === "number" ? data.per_page : 25,
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.actions.listRuns({
      owner: data.owner,
      name: data.name,
      page: data.page,
      per_page: data.per_page,
    });
  });

/** SSR: `repo.actions.listWorkflows` with Cookie forward. */
export const fetchActionsListWorkflows = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { git_ref?: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    git_ref: data?.git_ref ? String(data.git_ref) : undefined,
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.actions.listWorkflows({
      owner: data.owner,
      name: data.name,
      git_ref: data.git_ref,
    });
  });

/** SSR: `repo.actions.getRun` with Cookie forward. */
export const fetchActionsGetRun = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { run_id: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    run_id: String(data?.run_id ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.actions.getRun({
      owner: data.owner,
      name: data.name,
      run_id: data.run_id,
    });
  });

/** SSR: `repo.actions.getJobLog` with Cookie forward. */
export const fetchActionsGetJobLog = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { run_id: string; job_id: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    run_id: String(data?.run_id ?? ""),
    job_id: String(data?.job_id ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.actions.getJobLog({
      owner: data.owner,
      name: data.name,
      run_id: data.run_id,
      job_id: data.job_id,
    });
  });

/** SSR: `repo.stargazers.list` (Write+ gated). */
export const fetchRepoStargazers = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { q?: string; offset?: number; limit?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    q: data?.q ? String(data.q) : "",
    offset: Number(data?.offset ?? 0),
    limit: Number(data?.limit ?? 30),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.stargazersList({
      owner: data.owner,
      name: data.name,
      q: data.q.trim() || null,
      offset: data.offset,
      limit: data.limit,
    });
  });

/** SSR: `repo.watchers.list`. */
export const fetchRepoWatchers = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { q?: string; offset?: number; limit?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    q: data?.q ? String(data.q) : "",
    offset: Number(data?.offset ?? 0),
    limit: Number(data?.limit ?? 30),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.watchersList({
      owner: data.owner,
      name: data.name,
      q: data.q.trim() || null,
      offset: data.offset,
      limit: data.limit,
    });
  });

/** SSR: `repo.forks.list`. */
export const fetchRepoForks = createServerFn({ method: "GET" })
  .validator(
    (
      data: OwnerName & {
        q?: string;
        sort?: string;
        offset?: number;
        limit?: number;
      },
    ) => ({
      owner: String(data?.owner ?? ""),
      name: String(data?.name ?? ""),
      q: data?.q ? String(data.q) : "",
      sort: data?.sort ? String(data.sort) : "stars",
      offset: Number(data?.offset ?? 0),
      limit: Number(data?.limit ?? 30),
    }),
  )
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.forksList({
      owner: data.owner,
      name: data.name,
      q: data.q.trim() || null,
      sort: data.sort,
      offset: data.offset,
      limit: data.limit,
    });
  });

/** SSR: `repo.blame` with Cookie forward. */
export const fetchRepoBlame = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { ref: string; path: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    ref: String(data?.ref ?? ""),
    path: String(data?.path ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.blame({
      owner: data.owner,
      name: data.name,
      ref: data.ref,
      path: data.path,
    });
  });

/** SSR: `repo.commit` with Cookie forward. */
export const fetchRepoCommit = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { sha: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    sha: String(data?.sha ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.commit({
      owner: data.owner,
      name: data.name,
      sha: data.sha,
    });
  });

/** SSR: `repo.compare` with Cookie forward. */
export const fetchRepoCompare = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { base: string; head: string }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    base: String(data?.base ?? ""),
    head: String(data?.head ?? ""),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.repo.compare({
      owner: data.owner,
      name: data.name,
      base: data.base,
      head: data.head,
    });
  });

/** SSR: `issue.list` with Cookie forward. */
export const fetchIssueList = createServerFn({ method: "GET" })
  .validator(
    (
      data: OwnerName & {
        state?: string;
        author?: string;
        label?: string;
        assignee?: string;
        q?: string;
        offset?: number;
        limit?: number;
      },
    ) => ({
      owner: String(data?.owner ?? ""),
      name: String(data?.name ?? ""),
      state: data?.state ? String(data.state) : "open",
      author: data?.author ? String(data.author) : "",
      label: data?.label ? String(data.label) : "",
      assignee: data?.assignee ? String(data.assignee) : "",
      q: data?.q ? String(data.q) : "",
      offset: typeof data?.offset === "number" ? data.offset : 0,
      limit: typeof data?.limit === "number" ? data.limit : 25,
    }),
  )
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.issue.list({
      owner: data.owner,
      name: data.name,
      state: data.state,
      author: data.author || null,
      label: data.label || null,
      assignee: data.assignee || null,
      q: data.q || null,
      offset: data.offset,
      limit: data.limit,
    });
  });

/** SSR: `label.listForRepo` with Cookie forward. */
export const fetchLabelListForRepo = createServerFn({ method: "GET" })
  .validator(ownerNameValidator)
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.label.listForRepo({ owner: data.owner, name: data.name });
  });

/** SSR: `issue.get` with Cookie forward. */
export const fetchIssueGet = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { number: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    number: typeof data?.number === "number" ? data.number : Number(data?.number),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.issue.get({
      owner: data.owner,
      name: data.name,
      number: data.number,
    });
  });

/** SSR: `pull.get` with Cookie forward. */
export const fetchPullGet = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { number: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    number: typeof data?.number === "number" ? data.number : Number(data?.number),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.pull.get({
      owner: data.owner,
      name: data.name,
      number: data.number,
    });
  });

/** SSR: `pull.list` with Cookie forward. */
export const fetchPullList = createServerFn({ method: "GET" })
  .validator((data: OwnerName & { state?: string | null; offset?: number; limit?: number }) => ({
    owner: String(data?.owner ?? ""),
    name: String(data?.name ?? ""),
    state: data?.state == null || data.state === "" ? null : String(data.state),
    offset: typeof data?.offset === "number" ? data.offset : Number(data?.offset ?? 0),
    limit: typeof data?.limit === "number" ? data.limit : Number(data?.limit ?? 25),
  }))
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.pull.list({
      owner: data.owner,
      name: data.name,
      state: data.state,
      offset: data.offset,
      limit: data.limit,
    });
  });

/** SSR: `release.list` with Cookie forward. */
export const fetchReleaseList = createServerFn({ method: "GET" })
  .validator(ownerNameValidator)
  .handler(async ({ data }) => {
    const client = createSsrClient(incomingCookie());
    return client.release.list({ owner: data.owner, name: data.name });
  });

/**
 * SSR: browser-facing origin for clone URLs.
 * Prefer OXIDEAN_PUBLIC_ORIGIN; fall back to forwarded Host.
 */
export const fetchPublicOrigin = createServerFn({ method: "GET" }).handler(async () => {
  const fromEnv = resolvePublicOriginFromEnv();
  if (fromEnv) return fromEnv;

  const host =
    getRequestHeader("x-forwarded-host")?.split(",")[0]?.trim() ||
    getRequestHeader("host")?.trim() ||
    "";
  if (!host) {
    return "http://localhost";
  }

  const protoRaw =
    getRequestHeader("x-forwarded-proto")?.split(",")[0]?.trim() ||
    (host.startsWith("localhost") || host.startsWith("127.0.0.1") ? "http" : "https");
  const proto = protoRaw === "https" ? "https" : "http";
  return `${proto}://${host}`.replace(/\/$/, "");
});

/**
 * SSR: advertised Git SSH host + port for CloneBox.
 * Must be server-fn’d — browser bundles cannot read OXIDEAN_SSH_* at runtime.
 */
export const fetchSshAdvertise = createServerFn({ method: "GET" })
  .validator((data: { publicOrigin?: string }) => ({
    publicOrigin: typeof data?.publicOrigin === "string" ? data.publicOrigin : "",
  }))
  .handler(async ({ data }) => {
    const publicOrigin =
      data.publicOrigin.trim() || resolvePublicOriginFromEnv() || "http://localhost";
    return {
      sshHost: resolveSshHost(publicOrigin),
      sshPort: resolveSshPort(),
    };
  });
