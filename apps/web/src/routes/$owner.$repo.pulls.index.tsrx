import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useQuery } from "@octanejs/tanstack-query";
import type { PullListResponse, RepoPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { MemberLookup } from "@/components/org/member-lookup";
import { PullsList } from "@/components/repo/pulls-list";
import {
  repoListApplyClass,
  repoListFilterFieldClass,
  repoListFilterFormClass,
  repoListFilterLabelClass,
  repoListPrimaryCtaClass,
  repoListTabClass,
  repoListTablistClass,
} from "@/components/repo/repo-list-toolbar";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { apiClient } from "@/lib/api-client";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";
import { fetchPullList, fetchRepoGet } from "@/lib/ssr-repo";
import { useChromeAccountState } from "@/lib/use-chrome-account";
import { AppLink } from "@/components/ui/app-link";

export type PullsLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      list: PullListResponse;
      state: "open" | "closed" | "all";
    };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const PAGE_SIZE = 25;

async function loadPullsPage(
  owner: string,
  repoName: string,
  state: "open" | "closed" | "all",
): Promise<PullsLoaderData> {
  const [got, listRes] = await Promise.all([
    fetchRepoGet({ data: { owner, name: repoName } }),
    fetchPullList({
      data: {
        owner,
        name: repoName,
        state,
        offset: 0,
        limit: PAGE_SIZE,
      },
    }),
  ]);
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  if (!listRes.ok) {
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: listRes.error.message || "Could not load pull requests.",
    };
  }
  return {
    kind: "ready",
    repo: got.data,
    list: listRes.data,
    state,
  };
}

export const Route = createFileRoute("/$owner/$repo/pulls/")({
  component: PullsListPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Pulls · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<PullsLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    try {
      return await loadPullsPage(owner, repoName, "open");
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

export function PullsListPage() @{
  const params = useParams({ strict: false }) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const account = useChromeAccountState();
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const routeLoader = useLoaderData({ from: "/$owner/$repo/pulls/" }) as
    | PullsLoaderData
    | undefined;

  const [state, setState] = useState<"open" | "closed" | "all">("open");
  const [author, setAuthor] = useState("");
  const [assignee, setAssignee] = useState("");
  const [q, setQ] = useState("");
  const [reviewState, setReviewState] = useState("");
  const [appliedAuthor, setAppliedAuthor] = useState("");
  const [appliedAssignee, setAppliedAssignee] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [appliedReviewState, setAppliedReviewState] = useState("");

  const ssrListMatches =
    routeLoader?.kind === "ready" &&
    state === "open" &&
    !appliedAuthor &&
    !appliedAssignee &&
    !appliedQ &&
    !appliedReviewState;

  const listQ = useQuery({
    queryKey: [
      "pull",
      "list",
      owner,
      repoName,
      state,
      appliedAuthor,
      appliedAssignee,
      appliedQ,
      appliedReviewState,
    ] as const,
    queryFn: async () => {
      const res = await apiClient.pull.list({
        owner,
        name: repoName,
        state,
        author: appliedAuthor || null,
        assignee: appliedAssignee || null,
        q: appliedQ || null,
        review_state: appliedReviewState || null,
        offset: 0,
        limit: PAGE_SIZE,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName,
    initialData: ssrListMatches ? routeLoader.list : undefined,
  });

  const list = listQ.data ?? (ssrListMatches ? routeLoader.list : undefined);

  @if (routeLoader?.kind === "not_found" || layout?.status === "not_found") {
    <RepoNotFound />;
  } @else {
    @if (routeLoader?.kind === "error") {
      <p className="text-destructive">{routeLoader.message as string}</p>;
    } @else {
      <div className="space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className={repoListTablistClass() as string} role="tablist" aria-label="Pull state">
            <button
              type="button"
              className={repoListTabClass(state === "open") as string}
              onClick={() => setState("open")}
            >
              Open
            </button>
            <button
              type="button"
              className={repoListTabClass(state === "closed") as string}
              onClick={() => setState("closed")}
            >
              Closed
            </button>
            <button
              type="button"
              className={repoListTabClass(state === "all") as string}
              onClick={() => setState("all")}
            >
              All
            </button>
          </div>
          @if (account.user?.email_verified) {
            <AppLink
              href={`/${owner}/${repoName}/pulls/new` as string}
              className={repoListPrimaryCtaClass() as string}
            >
              New pull request
            </AppLink>;
          }
        </div>
        <form
          method="post"
          action="#"
          className={repoListFilterFormClass() as string}
          onSubmit={(e) => {
            e.preventDefault();
            setAppliedAuthor(author.trim());
            setAppliedAssignee(assignee.trim());
            setAppliedQ(q.trim());
            setAppliedReviewState(reviewState);
          }}
        >
          <MemberLookup
            id="pull-filter-author"
            name="author"
            label="Author"
            compact
            className="min-w-[9rem]"
            placeholder="username"
            value={author as string}
            onValueChange={setAuthor}
          />
          <MemberLookup
            id="pull-filter-assignee"
            name="assignee"
            label="Assignee"
            compact
            className="min-w-[9rem]"
            placeholder="username"
            value={assignee as string}
            onValueChange={setAssignee}
          />
          <label className={repoListFilterLabelClass() as string}>
            Review state
            <select
              className={repoListFilterFieldClass() as string}
              value={reviewState as string}
              onChange={(e: Event) => setReviewState((e.target as HTMLSelectElement).value)}
            >
              <option value="">Any</option>
              <option value="approved">Approved</option>
              <option value="changes_requested">Changes requested</option>
              <option value="review_required">Review required</option>
            </select>
          </label>
          <label className={`${repoListFilterLabelClass()} min-w-[12rem] flex-1` as string}>
            Search
            <input
              type="search"
              name="q"
              autoComplete="off"
              placeholder="Search title or body"
              className={`${repoListFilterFieldClass()} w-full` as string}
              value={q as string}
              onInput={(e: Event) => setQ((e.target as HTMLInputElement).value)}
            />
          </label>
          <button type="submit" className={repoListApplyClass() as string}>
            Apply
          </button>
        </form>
        @if (list) {
          <PullsList
            owner={owner as string}
            repo={repoName as string}
            list={list as PullListResponse}
          />;
        } @else {
          <p className="text-muted-foreground">Loading pull requests…</p>;
        }
      </div>;
    }
  }
}
