import { createFileRoute, useNavigate, useParams, useSearch } from "@octanejs/tanstack-router";
import { useQuery } from "@octanejs/tanstack-query";
import { useState } from "octane";
import type { RepoSearchHit, RepoSearchType } from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";
import { CodeSnippet } from "@/components/code-snippet";
import { AppLink } from "@/components/ui/app-link";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

type SearchSearch = {
  q?: string;
  type?: string;
};

const TYPE_TABS: { id: RepoSearchType; label: string }[] = [
  { id: "code", label: "Code" },
  { id: "commits", label: "Commits" },
  { id: "issues", label: "Issues" },
  { id: "pulls", label: "Pull requests" },
];

function parseType(raw: string | undefined): RepoSearchType {
  switch ((raw ?? "code").toLowerCase()) {
    case "commits":
      return "commits";
    case "issues":
      return "issues";
    case "pulls":
      return "pulls";
    default:
      return "code";
  }
}

function hitKey(hit: RepoSearchHit): string {
  switch (hit.kind) {
    case "code":
      return `code-${hit.path}-${hit.line}`;
    case "commit":
      return `commit-${hit.sha}`;
    case "issue":
      return `issue-${hit.number}`;
    case "pull":
      return `pull-${hit.number}`;
  }
}

export const Route = createFileRoute("/$owner/$repo/search")({
  validateSearch: (search: Record<string, unknown>): SearchSearch => ({
    q: typeof search.q === "string" ? search.q : undefined,
    type: typeof search.type === "string" ? search.type : undefined,
  }),
  component: RepoSearchPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Search · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
});

/** In-repo search results — type tabs + hit lists (D-SRCH-02, D-SRCH-15). */
export function RepoSearchPage() @{
  const params = useParams({ strict: false }) as { owner?: string; repo?: string };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const search = useSearch({ strict: false }) as SearchSearch;
  const navigate = useNavigate();
  const qParam = search.q ?? "";
  const typeParam = parseType(search.type);
  const [draft, setDraft] = useState(qParam);

  const searchQ = useQuery({
    queryKey: ["repo", "search", owner, repoName, typeParam, qParam] as const,
    queryFn: async () => {
      const res = await apiClient.repo.search({
        owner,
        name: repoName,
        type: typeParam,
        q: qParam,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName,
  });

  const hits: RepoSearchHit[] = searchQ.data?.hits ?? [];
  const truncated = searchQ.data?.truncated ?? false;

  function go(next: { q?: string; type?: RepoSearchType }) {
    void navigate({
      to: "/$owner/$repo/search",
      params: { owner, repo: repoName },
      search: {
        q: next.q ?? qParam,
        type: next.type ?? typeParam,
      },
    });
  }

  function submitSearch() {
    go({ q: draft, type: typeParam });
  }

  function tabClass(id: RepoSearchType): string {
    return id === typeParam
      ? "border-b-2 border-primary pb-2 text-[14px] font-semibold text-foreground"
      : "pb-2 text-[14px] text-muted-foreground hover:text-foreground";
  }

  @{
    <div className="space-y-4" data-testid="repo-search-page">
      <h2 className="text-[18px] font-semibold text-foreground">Search</h2>
      <form
        method="post"
        action="#"
        className="flex flex-wrap gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          submitSearch();
        }}
      >
        <label className="sr-only" for="repo-search-q">
          Search query
        </label>
        <input
          id="repo-search-q"
          name="q"
          type="search"
          className="min-w-[16rem] flex-1 rounded-md border border-border bg-background px-3 py-2 text-[14px] text-foreground"
          value={draft as string}
          placeholder="Search this repository"
          onInput={(e) => setDraft((e.target as HTMLInputElement).value)}
        />
        <button
          type="submit"
          className="rounded-md bg-primary px-3 py-2 text-[14px] font-medium text-primary-foreground"
        >
          Search
        </button>
      </form>

      <nav className="flex flex-wrap gap-4 border-b border-border" aria-label="Search type">
        @for (const tab of TYPE_TABS; key tab.id) {
          <button
            type="button"
            className={tabClass(tab.id) as string}
            aria-current={tab.id === typeParam ? "page" : undefined}
            data-testid={`search-tab-${tab.id}`}
            onClick={() => go({ type: tab.id })}
          >
            {tab.label as string}
          </button>;
        }
      </nav>

      @if (searchQ.isError) {
        <p role="alert" className="text-[16px] text-destructive">
          {(searchQ.error?.message || NETWORK_ERROR) as string}
        </p>;
      } @else if (searchQ.isPending && qParam) {
        <p className="text-[16px] text-muted-foreground">Searching…</p>;
      } @else if (!qParam) {
        <p className="text-[16px] text-muted-foreground">
          Enter a query to search this repository.
        </p>;
      } @else if (hits.length === 0) {
        <p className="text-[16px] text-muted-foreground" data-testid="search-empty">
          No results for that query.
        </p>;
      } @else {
        <>
          @if (truncated) {
            <p
              className="text-[13px] text-muted-foreground"
              data-testid="search-truncated"
            >
              Results truncated.
            </p>;
          }
          <ul className="divide-y divide-border rounded-md border border-border">
            @for (const hit of hits; key hitKey(hit)) {
              @if (hit.kind === "code") {
                <li className="px-3 py-2 text-[14px]">
                  <AppLink
                    className="font-medium text-primary underline"
                    href={`/${owner}/${repoName}/blob/${hit.path}` as string}
                  >
                    {hit.path as string}
                  </AppLink>
                  @if (hit.line > 0) {
                    <span className="text-muted-foreground">
                      :{String(hit.line)}
                    </span>;
                    <CodeSnippet code={hit.content} path={hit.path} />;
                  }
                </li>;
              } @else {
                @if (hit.kind === "commit") {
                  <li className="px-3 py-2 text-[14px]">
                    <AppLink
                      className="font-medium text-primary underline"
                      href={`/${owner}/${repoName}/commit/${hit.sha}` as string}
                    >
                      {hit.short_sha as string}
                    </AppLink>
                    <span className="ml-2 text-foreground">
                      {hit.subject as string}
                    </span>
                    <p className="text-[12px] text-muted-foreground">
                      {(hit.author_name + " · " + hit.authored_at) as string}
                    </p>
                  </li>;
                } @else {
                  @if (hit.kind === "issue") {
                    <li className="px-3 py-2 text-[14px]">
                      <AppLink
                        className="font-medium text-primary underline"
                        href={`/${owner}/${repoName}/issues/${hit.number}` as string}
                      >
                        {"#" + String(hit.number) + " " + hit.title}
                      </AppLink>
                      <span className="ml-2 text-[12px] text-muted-foreground">
                        {hit.state as string}
                      </span>
                    </li>;
                  } @else {
                    <li className="px-3 py-2 text-[14px]">
                      <AppLink
                        className="font-medium text-primary underline"
                        href={`/${owner}/${repoName}/pull/${hit.number}` as string}
                      >
                        {"#" + String(hit.number) + " " + hit.title}
                      </AppLink>
                      <span className="ml-2 text-[12px] text-muted-foreground">
                        {hit.state as string}
                      </span>
                    </li>;
                  }
                }
              }
            }
          </ul>
        </>;
      }
    </div>;
    };
}
