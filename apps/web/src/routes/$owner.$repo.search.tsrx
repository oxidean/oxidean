import { useQuery } from "@octanejs/tanstack-query";
import { navigate as astroNavigate } from "astro:transitions/client";

import { matchPath, routeHref, searchRecord } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useState } from "octane";
import type { RepoSearchType } from "@oxidean/api-client";
import type { HighlightTheme } from "@/lib/highlight";
import { fetchRepoSearch, type SsrRepoSearchHit } from "@/lib/ssr-repo";
import { CodeSnippet } from "@/components/code-snippet";
import { AppLink } from "@/components/ui/app-link";
import { useRepoStore } from "@/lib/repo-store";

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

function hitKey(hit: SsrRepoSearchHit): string {
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

/** In-repo search results — type tabs + hit lists (D-SRCH-02, D-SRCH-15). */
function validateSearch(search: Record<string, unknown>): SearchSearch {
  return {
    q: typeof search.q === "string" ? search.q : undefined,
    type: typeof search.type === "string" ? search.type : undefined,
  };
}

function RepoSearchPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/search", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const search = validateSearch(searchRecord(searchStr)) as SearchSearch;
  const navigate = astroNavigate;
  const qParam = search.q ?? "";
  const typeParam = parseType(search.type);
  const [draft, setDraft] = useState(qParam);
  // Unit toggles (COL-13): hide Issues/Pulls result tabs when off. A deep link
  // to a disabled type still answers with the `repo.<unit>.disabled` message.
  const storeRepo = useRepoStore((s) => s.repo);
  const typeTabs = TYPE_TABS.filter((t) =>
    t.id === "issues"
      ? storeRepo?.issues_enabled !== false
      : t.id === "pulls"
        ? storeRepo?.pulls_enabled !== false
        : true,
  );

  const searchQ = useQuery({
    queryKey: ["repo", "search", owner, repoName, typeParam, qParam] as const,
    queryFn: async () => {
      const res = await fetchRepoSearch({ owner, name: repoName, type: typeParam, q: qParam });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res;
    },
    enabled: !!owner && !!repoName,
  });

  const hits: SsrRepoSearchHit[] = searchQ.data?.data?.hits ?? [];
  const truncated = searchQ.data?.data?.truncated ?? false;
  const highlightTheme: HighlightTheme | null = searchQ.data?.highlightTheme ?? null;

  function go(next: { q?: string; type?: RepoSearchType }) {
    void navigate(
      routeHref(
        "/$owner/$repo/search",
        { owner, repo: repoName },
        {
          q: next.q ?? qParam,
          type: next.type ?? typeParam,
        },
      ),
    );
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
        @for (const tab of typeTabs; key tab.id) {
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
                    <>
                      <span className="text-muted-foreground">
                        :{String(hit.line)}
                      </span>
                      <CodeSnippet
                        code={hit.content}
                        path={hit.path}
                        highlightedHtml={hit.html ?? null}
                        highlightTheme={highlightTheme}
                      />
                    </>;
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

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoSearchPage() @{
  <AppPage>
    <RepoLayout>
      <RepoSearchPageContent />
    </RepoLayout>
  </AppPage>;
}
