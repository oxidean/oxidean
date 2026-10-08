import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { useEffect, useState } from "octane";
import type {
  GlobalSearchCodeHit,
  GlobalSearchCommitHit,
  GlobalSearchGroup,
  GlobalSearchIssueHit,
  GlobalSearchKind,
  GlobalSearchOrgHit,
  GlobalSearchPullHit,
  GlobalSearchRepoHit,
  GlobalSearchResponse,
  GlobalSearchUserHit,
} from "@oxidean/api-client";
import {
  BookMarked,
  Building2,
  CircleDot,
  Code,
  GitCommitHorizontal,
  GitPullRequest,
  Search,
  Users,
} from "@octanejs/lucide";
import { CodeSnippet } from "@/components/code-snippet";
import { apiClient } from "@/lib/api-client";
import { highlightCode, languageIdForPath, type HighlightTheme } from "@/lib/highlight";
import { useSearchString } from "@/lib/pathname.tsrx";
import { resolveDocumentHighlightTheme } from "@/lib/theme";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { cn } from "@/lib/utils";
import { AppPage } from "@/components/app-page";
import { AppLink } from "@/components/ui/app-link";

type TabId = "overview" | GlobalSearchKind;

const TABS: { id: TabId; label: string }[] = [
  { id: "overview", label: "All results" },
  { id: "repositories", label: "Repositories" },
  { id: "code", label: "Code" },
  { id: "commits", label: "Commits" },
  { id: "issues", label: "Issues" },
  { id: "pulls", label: "Pull requests" },
  { id: "users", label: "Users" },
  { id: "organizations", label: "Organizations" },
];

const OVERVIEW_LIMIT = 5;
const TAB_LIMIT = 20;

/** Git-scan kinds only produce hits when requested (bounded repo scan). */
const SCAN_KINDS: ReadonlySet<TabId> = new Set(["code", "commits"]);

function parseType(raw: string | null | undefined): TabId {
  const t = (raw ?? "").toLowerCase();
  if (TABS.some((x) => x.id === t)) return t as TabId;
  return "overview";
}

/** Code hit carrying pre-rendered Shiki HTML when the server highlighted it. */
export type HighlightedCodeHit = GlobalSearchCodeHit & { html?: string };

/** GlobalSearchResponse with code hits optionally carrying SSR Shiki HTML. */
type SearchResults = Omit<GlobalSearchResponse, "code"> & {
  code: GlobalSearchGroup<HighlightedCodeHit>;
};

type SearchLoaderData = {
  q: string;
  type: TabId;
  offset: number;
  results: SearchResults | null;
  /** Theme used for SSR-highlighted code hits (null → not highlighted server-side). */
  highlightTheme: HighlightTheme | null;
  error: string | null;
  signedIn: boolean;
};

/**
 * Global search — one `search.global` RPC, grouped by entity. Code hits are
 * highlighted with the document theme (was SSR Shiki; now runs in the query).
 */
function searchQueryOptions(qRaw: string, typeRaw: string, offsetRaw: number) {
  const q = qRaw.trim();
  const type = parseType(typeRaw);
  const offset = Number.isFinite(offsetRaw) ? Math.max(0, Math.floor(offsetRaw)) : 0;
  return queryOptions({
    queryKey: ["search", "global", q, type, offset],
    queryFn: async (): Promise<SearchLoaderData> => {
      const me = await apiClient.auth.me();
      const signedIn = me.ok;

      if (!q) {
        return { q, type, offset, results: null, highlightTheme: null, error: null, signedIn };
      }

      const res = await apiClient.search.global({
        q,
        // `types` limits the expensive commits/code git scans; database-backed
        // groups always report `total` either way (tab badges).
        types: type === "overview" ? null : [type as GlobalSearchKind],
        offset,
        limit: type === "overview" ? OVERVIEW_LIMIT : TAB_LIMIT,
      });
      if (!res.ok) {
        return {
          q,
          type,
          offset,
          results: null,
          highlightTheme: null,
          error: res.error.message,
          signedIn,
        };
      }

      const codeHits = res.data.code.hits;
      if (codeHits.length === 0) {
        return {
          q,
          type,
          offset,
          results: res.data,
          highlightTheme: null,
          error: null,
          signedIn,
        };
      }
      const hits = codeHits.map((hit): HighlightedCodeHit => ({ ...hit }));
      let highlightTheme: HighlightTheme | null = null;
      try {
        const theme = resolveDocumentHighlightTheme();
        const htmls = await Promise.all(
          hits.map(async (hit) => {
            try {
              return await highlightCode(hit.content, {
                lang: languageIdForPath(hit.path),
                theme,
              });
            } catch {
              return undefined;
            }
          }),
        );
        hits.forEach((hit, i) => {
          hit.html = htmls[i];
        });
        highlightTheme = theme;
      } catch {
        highlightTheme = null;
      }
      const results: SearchResults = {
        ...res.data,
        code: { ...res.data.code, hits },
      };
      return { q, type, offset, results, highlightTheme, error: null, signedIn };
    },
    retry: false,
    staleTime: 30_000,
  });
}

function tabHref(q: string, type: TabId, offset = 0): string {
  const p = new URLSearchParams();
  if (q.trim()) p.set("q", q.trim());
  if (type !== "overview") p.set("type", type);
  if (offset > 0) p.set("offset", String(offset));
  const s = p.toString();
  return s ? `/search?${s}` : "/search";
}

function tabClass(active: boolean): string {
  return cn(
    "border-b-2 px-1 pb-2 text-[14px] no-underline",
    active
      ? "border-primary font-semibold text-foreground"
      : "border-transparent font-normal text-muted-foreground hover:text-foreground",
  );
}

function groupFor<T>(results: GlobalSearchResponse | null, id: TabId): GlobalSearchGroup<T> | null {
  if (!results) return null;
  switch (id) {
    case "repositories":
      return results.repositories as GlobalSearchGroup<T>;
    case "users":
      return results.users as GlobalSearchGroup<T>;
    case "organizations":
      return results.organizations as GlobalSearchGroup<T>;
    case "issues":
      return results.issues as GlobalSearchGroup<T>;
    case "pulls":
      return results.pulls as GlobalSearchGroup<T>;
    case "commits":
      return results.commits as GlobalSearchGroup<T>;
    case "code":
      return results.code as GlobalSearchGroup<T>;
    default:
      return null;
  }
}

/**
 * Tab badge: exact COUNT for SQL-backed groups. Commits/code only scan the
 * newest visible repos, so their totals are only meaningful when that group
 * was populated (active tab or the overview) — and `truncated` → "N+".
 */
function tabBadge(data: SearchLoaderData | undefined, id: TabId): string | null {
  if (id === "overview") return null;
  const g = groupFor<unknown>(data?.results ?? null, id);
  if (!g) return null;
  if (SCAN_KINDS.has(id) && data?.type !== id && data?.type !== "overview") return null;
  if (g.total <= 0) return null;
  return g.truncated && SCAN_KINDS.has(id) ? `${g.total}+` : `${g.total}`;
}

function issueKey(h: GlobalSearchIssueHit): string {
  return `${h.repo_owner}/${h.repo_name}#${h.number}`;
}
function pullKey(h: GlobalSearchPullHit): string {
  return `${h.repo_owner}/${h.repo_name}#${h.number}`;
}
function commitKey(h: GlobalSearchCommitHit): string {
  return `${h.repo_owner}/${h.repo_name}@${h.sha}`;
}
function codeKey(h: GlobalSearchCodeHit): string {
  return `${h.repo_owner}/${h.repo_name}:${h.path}:${h.line}`;
}
function repoKey(h: GlobalSearchRepoHit): string {
  return `${h.owner}/${h.name}`;
}

function stateBadgeVariant(state: string): "open" | "closed" | "merged" | "draft" {
  if (state === "open") return "open";
  if (state === "merged") return "merged";
  return "closed";
}

function repoFullName(owner: string, name: string): string {
  return `${owner}/${name}`;
}

function RepoRow({ hit }: { hit: GlobalSearchRepoHit }) @{
  <ListRow className="items-start gap-3 py-4">
    <BookMarked aria-hidden size={18} className="mt-0.5 shrink-0 text-muted-foreground" />
    <div className="min-w-0 flex-1">
      <div className="flex flex-wrap items-center gap-2">
        <AppLink
          href={("/" + hit.owner + "/" + hit.name) as string}
          className="text-[16px] font-semibold text-primary no-underline hover:underline"
        >
          {repoFullName(hit.owner, hit.name) as string}
        </AppLink>
        <Badge variant={hit.visibility === "private" ? "outline" : "success"}>
          {(hit.visibility === "private" ? "Private" : "Public") as string}
        </Badge>
      </div>
      @if (hit.description?.trim()) {
        <p className="mt-1 text-[14px] text-muted-foreground">{hit.description as string}</p>;
      }
      <p className="mt-1 text-[12px] text-muted-foreground">
        {(String(hit.star_count) + (hit.star_count === 1 ? " star" : " stars")) as string}
      </p>
    </div>
  </ListRow>;
}

function UserRow({ hit }: { hit: GlobalSearchUserHit }) @{
  <ListRow className="gap-2">
    <Users aria-hidden size={16} className="shrink-0 text-muted-foreground" />
    <AppLink
      href={("/" + hit.username) as string}
      className="text-[15px] font-semibold text-primary no-underline hover:underline"
    >
      {("@" + hit.username) as string}
    </AppLink>
    @if (hit.display_name) {
      <span className="text-[14px] text-muted-foreground">{hit.display_name as string}</span>;
    }
  </ListRow>;
}

function OrgRow({ hit }: { hit: GlobalSearchOrgHit }) @{
  <ListRow className="gap-2">
    <Building2 aria-hidden size={16} className="shrink-0 text-muted-foreground" />
    <AppLink
      href={("/" + hit.slug) as string}
      className="text-[15px] font-semibold text-primary no-underline hover:underline"
    >
      {hit.slug as string}
    </AppLink>
    @if (hit.display_name) {
      <span className="text-[14px] text-muted-foreground">{hit.display_name as string}</span>;
    }
  </ListRow>;
}

function IssueRow({ hit }: { hit: GlobalSearchIssueHit }) @{
  <ListRow className="items-start gap-3">
    <CircleDot aria-hidden size={16} className="mt-1 shrink-0 text-muted-foreground" />
    <div className="min-w-0 flex-1">
      <AppLink
        href={("/" + hit.repo_owner + "/" + hit.repo_name + "/issues/" + hit.number) as string}
        className="text-[15px] font-medium text-foreground no-underline hover:text-primary hover:underline"
      >
        {hit.title as string}
      </AppLink>
      <p className="mt-0.5 text-[13px] text-muted-foreground">
        {
          (repoFullName(hit.repo_owner, hit.repo_name) +
            " #" +
            hit.number +
            (hit.author_username ? " · " + hit.author_username : "") +
            (hit.comment_count > 0 ? " · " + hit.comment_count + " comments" : "")) as string
        }
      </p>
    </div>
    <Badge variant={stateBadgeVariant(hit.state)} className="mt-1 shrink-0">
      {hit.state as string}
    </Badge>
  </ListRow>;
}

function PullRow({ hit }: { hit: GlobalSearchPullHit }) @{
  <ListRow className="items-start gap-3">
    <GitPullRequest aria-hidden size={16} className="mt-1 shrink-0 text-muted-foreground" />
    <div className="min-w-0 flex-1">
      <AppLink
        href={("/" + hit.repo_owner + "/" + hit.repo_name + "/pull/" + hit.number) as string}
        className="text-[15px] font-medium text-foreground no-underline hover:text-primary hover:underline"
      >
        {hit.title as string}
      </AppLink>
      <p className="mt-0.5 text-[13px] text-muted-foreground">
        {
          (repoFullName(hit.repo_owner, hit.repo_name) +
            " #" +
            hit.number +
            (hit.author_username ? " · " + hit.author_username : "") +
            (hit.comment_count > 0 ? " · " + hit.comment_count + " comments" : "")) as string
        }
      </p>
    </div>
    @if (hit.draft) {
      <Badge variant="draft" className="mt-1 shrink-0">
        {"Draft" as string}
      </Badge>;
    }
    <Badge variant={stateBadgeVariant(hit.state)} className="mt-1 shrink-0">
      {hit.state as string}
    </Badge>
  </ListRow>;
}

function CommitRow({ hit }: { hit: GlobalSearchCommitHit }) @{
  <ListRow className="items-start gap-3">
    <GitCommitHorizontal aria-hidden size={16} className="mt-1 shrink-0 text-muted-foreground" />
    <div className="min-w-0 flex-1">
      <AppLink
        href={("/" + hit.repo_owner + "/" + hit.repo_name + "/commit/" + hit.sha) as string}
        className="font-mono text-[13px] font-medium text-primary no-underline hover:underline"
      >
        {hit.short_sha as string}
      </AppLink>
      <span className="ml-2 text-[14px] text-foreground">{hit.subject as string}</span>
      <p className="mt-0.5 text-[13px] text-muted-foreground">
        {
          (repoFullName(hit.repo_owner, hit.repo_name) +
            " · " +
            hit.author_name +
            " · " +
            hit.authored_at) as string
        }
      </p>
    </div>
  </ListRow>;
}

function CodeRow({
  hit,
  highlightTheme,
}: {
  hit: HighlightedCodeHit;
  highlightTheme: HighlightTheme | null;
}) @{
  <ListRow className="block px-4 py-3">
    <AppLink
      href={("/" + hit.repo_owner + "/" + hit.repo_name + "/blob/" + hit.path) as string}
      className="font-mono text-[13px] font-medium text-primary no-underline hover:underline"
    >
      {(repoFullName(hit.repo_owner, hit.repo_name) + ":" + hit.path) as string}
    </AppLink>
    <span className="font-mono text-[13px] text-muted-foreground">
      {(":" + hit.line) as string}
    </span>
    <CodeSnippet
      code={hit.content}
      path={hit.path}
      highlightedHtml={hit.html ?? null}
      highlightTheme={highlightTheme}
    />
  </ListRow>;
}

/** Grouped section header used on the overview + tab pages. */
function SectionHeader({
  icon,
  title,
  count,
  moreHref,
}: {
  icon: unknown;
  title: string;
  count: number;
  moreHref: string | null;
}) @{
  <div className="flex items-baseline justify-between gap-3">
    <h2 className="flex items-center gap-2 text-[15px] font-semibold text-foreground">
      {icon}
      {title as string}
      <span className="text-[13px] font-normal text-muted-foreground">
        {String(count) as string}
      </span>
    </h2>
    @if (moreHref) {
      <AppLink
        href={moreHref as string}
        className="text-[13px] text-muted-foreground no-underline hover:text-foreground hover:underline"
      >
        {"View all" as string}
      </AppLink>;
    }
  </div>;
}

/** `/search` — sitewide results (title/meta via shell). */
export function SearchPage() @{
  <AppPage>
    <SearchContent />
  </AppPage>;
}

/** Sitewide search results — one `search.global` RPC, grouped by entity. */
export function SearchContent() @{
  const searchStr = useSearchString();
  const params = new URLSearchParams(searchStr);
  const qParam = params.get("q") ?? "";
  const typeParam = params.get("type") ?? "";
  const offsetParam = Number(params.get("offset") ?? "0");
  const searchQ = useQuery(searchQueryOptions(qParam, typeParam, offsetParam));
  const data = searchQ.data;
  const q0 = data?.q ?? qParam.trim();
  const type: TabId = data?.type ?? parseType(typeParam);
  const offset = data?.offset ?? (Number.isFinite(offsetParam) ? Math.max(0, offsetParam) : 0);

  // Query-aware document title (shell carries the static fallback).
  useEffect(() => {
    const t = q0.trim();
    document.title = t ? `Search · ${t} · Oxidean` : "Search · Oxidean";
  }, [q0]);
  const results = data?.results ?? null;
  const signedIn = data?.signedIn === true;
  const [q, setQ] = useState(q0);

  const repos = results?.repositories;
  const usersG = results?.users;
  const orgsG = results?.organizations;
  const issuesG = results?.issues;
  const pullsG = results?.pulls;
  const commitsG = results?.commits;
  const codeG = results?.code;

  const overviewEmpty =
    !!results &&
    (repos?.hits.length ?? 0) === 0 &&
    (usersG?.hits.length ?? 0) === 0 &&
    (orgsG?.hits.length ?? 0) === 0 &&
    (issuesG?.hits.length ?? 0) === 0 &&
    (pullsG?.hits.length ?? 0) === 0 &&
    (commitsG?.hits.length ?? 0) === 0 &&
    (codeG?.hits.length ?? 0) === 0;

  const activeGroup = groupFor<unknown>(results, type);
  const hasPrev = offset > 0;
  const hasNext =
    !!activeGroup &&
    (offset + activeGroup.hits.length < activeGroup.total || activeGroup.truncated);

  <div className="mx-auto max-w-5xl px-4 py-10 sm:px-6" data-testid="search-page">
    <PageHeader
      icon={<Search aria-hidden size={22} />}
      title="Search"
      description="Repositories, code, commits, issues, pull requests, users, and organizations — scoped to what you can access."
      divider={false}
    />

    <form
      method="get"
      action="/search"
      className="mt-6 flex max-w-xl gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        window.location.assign(tabHref(q, type));
      }}
    >
      <div className="relative flex-1">
        <Search
          aria-hidden
          size={16}
          className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          name="q"
          value={q as string}
          onInput={(e) => setQ((e.currentTarget as HTMLInputElement).value)}
          placeholder="Search Oxidean…"
          className="pl-8"
          aria-label="Search query"
          data-testid="search-page-input"
        />
      </div>
      @if (type !== "overview") {
        <input type="hidden" name="type" value={type as string} />;
      }
      <Button type="submit">Search</Button>
    </form>

    <nav
      className="mt-8 flex flex-wrap gap-4 border-b border-border"
      aria-label="Search categories"
    >
      @for (const tab of TABS; key tab.id) {
        const badge = tabBadge(data, tab.id);
        <AppLink
          href={tabHref(q0, tab.id) as string}
          className={tabClass(type === tab.id) as string}
          aria-current={type === tab.id ? "page" : undefined}
          activeOptions={{ exact: true }}
        >
          {tab.label as string}
          @if (badge) {
            <span className="ml-1 rounded-full bg-muted px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground">
              {badge as string}
            </span>;
          }
        </AppLink>;
      }
    </nav>

    <div className="mt-8">
      @if (!q0) {
        <EmptyState
          icon={<Search aria-hidden size={22} />}
          title="Search Oxidean"
          description="Type a query to search repositories, code, commits, issues, pull requests, users, and organizations you can access."
        />;
      } @else if (data?.error) {
        <p className="text-[15px] text-muted-foreground" data-testid="search-error">
          {(data?.error ?? "") as string}
        </p>;
      } @else if (type === "overview") {
        @if (overviewEmpty) {
          <EmptyState
            icon={<Search aria-hidden size={22} />}
            title="No results"
            description="Nothing matched your search across repositories, code, commits, issues, pull requests, users, or organizations."
          />;
        } @else {
          <>
            @if (repos && repos.hits.length > 0) {
              <section data-testid="search-section-repositories">
                <SectionHeader
                  icon={<BookMarked aria-hidden size={16} />}
                  title="Repositories"
                  count={repos.total}
                  moreHref={
                    repos.total > repos.hits.length ? tabHref(q0, "repositories") : null
                  }
                />
                <List className="mt-2">
                  @for (const hit of repos.hits; key repoKey(hit)) {
                    <RepoRow hit={hit} />;
                  }
                </List>
              </section>;
            }
            @if (issuesG && issuesG.hits.length > 0) {
              <section className="mt-8" data-testid="search-section-issues">
                <SectionHeader
                  icon={<CircleDot aria-hidden size={16} />}
                  title="Issues"
                  count={issuesG.total}
                  moreHref={
                    issuesG.total > issuesG.hits.length ? tabHref(q0, "issues") : null
                  }
                />
                <List className="mt-2">
                  @for (const hit of issuesG.hits; key issueKey(hit)) {
                    <IssueRow hit={hit} />;
                  }
                </List>
              </section>;
            }
            @if (pullsG && pullsG.hits.length > 0) {
              <section className="mt-8" data-testid="search-section-pulls">
                <SectionHeader
                  icon={<GitPullRequest aria-hidden size={16} />}
                  title="Pull requests"
                  count={pullsG.total}
                  moreHref={
                    pullsG.total > pullsG.hits.length ? tabHref(q0, "pulls") : null
                  }
                />
                <List className="mt-2">
                  @for (const hit of pullsG.hits; key pullKey(hit)) {
                    <PullRow hit={hit} />;
                  }
                </List>
              </section>;
            }
            @if (commitsG && commitsG.hits.length > 0) {
              <section className="mt-8" data-testid="search-section-commits">
                <SectionHeader
                  icon={<GitCommitHorizontal aria-hidden size={16} />}
                  title="Commits"
                  count={commitsG.total}
                  moreHref={tabHref(q0, "commits")}
                />
                <List className="mt-2">
                  @for (const hit of commitsG.hits; key commitKey(hit)) {
                    <CommitRow hit={hit} />;
                  }
                </List>
                @if (commitsG.truncated) {
                  <p className="mt-2 text-[12px] text-muted-foreground">
                    Showing hits found in your most recently updated repositories.
                  </p>;
                }
              </section>;
            }
            @if (codeG && codeG.hits.length > 0) {
              <section className="mt-8" data-testid="search-section-code">
                <SectionHeader
                  icon={<Code aria-hidden size={16} />}
                  title="Code"
                  count={codeG.total}
                  moreHref={tabHref(q0, "code")}
                />
                <List className="mt-2">
                  @for (const hit of codeG.hits; key codeKey(hit)) {
                    <CodeRow
                      hit={hit}
                      highlightTheme={data?.highlightTheme ?? null}
                    />;
                  }
                </List>
                <p className="mt-2 text-[12px] text-muted-foreground">
                  {
                    (codeG.truncated
                      ? "Partial coverage — code search scans your most recently updated repositories."
                      : "Code search covers your most recently updated repositories.") as string
                  }
                </p>
              </section>;
            }
            @if (usersG && usersG.hits.length > 0) {
              <section className="mt-8" data-testid="search-section-users">
                <SectionHeader
                  icon={<Users aria-hidden size={16} />}
                  title="Users"
                  count={usersG.total}
                  moreHref={
                    usersG.total > usersG.hits.length ? tabHref(q0, "users") : null
                  }
                />
                <List className="mt-2">
                  @for (const hit of usersG.hits; key hit.username) {
                    <UserRow hit={hit} />;
                  }
                </List>
              </section>;
            }
            @if (orgsG && orgsG.hits.length > 0) {
              <section className="mt-8" data-testid="search-section-organizations">
                <SectionHeader
                  icon={<Building2 aria-hidden size={16} />}
                  title="Organizations"
                  count={orgsG.total}
                  moreHref={
                    orgsG.total > orgsG.hits.length
                      ? tabHref(q0, "organizations")
                      : null
                  }
                />
                <List className="mt-2">
                  @for (const hit of orgsG.hits; key hit.slug) {
                    <OrgRow hit={hit} />;
                  }
                </List>
              </section>;
            }
          </>;
        }
      } @else {
        <>
          @if (type === "users" && !signedIn) {
            <p className="text-[15px] text-muted-foreground">
              <AppLink
                href="/login?returnTo=/search"
                className="text-primary hover:underline"
              >
                Sign in
              </AppLink>{" "}
              to search users.
            </p>;
          } @else if (!activeGroup || activeGroup.hits.length === 0) {
            <EmptyState
              icon={<Search aria-hidden size={22} />}
              title="No results"
              description={
                SCAN_KINDS.has(type)
                  ? "No matches in the repositories scanned (your most recently updated, readable repos)."
                  : "Nothing matched your search. Try different keywords."
              }
            />;
          } @else {
            <>
              @if (SCAN_KINDS.has(type)) {
                <p
                  className="mb-3 text-[13px] text-muted-foreground"
                  data-testid="search-scan-note"
                >
                  {
                    "Scanning the most recently updated repositories you can read — instance-wide indexed search is planned." as string
                  }
                </p>;
              }
              <List>
                @if (type === "repositories") {
                  @for (const hit of repos?.hits ?? []; key repoKey(hit)) {
                    <RepoRow hit={hit} />;
                  }
                }
                @if (type === "issues") {
                  @for (const hit of issuesG?.hits ?? []; key issueKey(hit)) {
                    <IssueRow hit={hit} />;
                  }
                }
                @if (type === "pulls") {
                  @for (const hit of pullsG?.hits ?? []; key pullKey(hit)) {
                    <PullRow hit={hit} />;
                  }
                }
                @if (type === "commits") {
                  @for (const hit of commitsG?.hits ?? []; key commitKey(hit)) {
                    <CommitRow hit={hit} />;
                  }
                }
                @if (type === "code") {
                  @for (const hit of codeG?.hits ?? []; key codeKey(hit)) {
                    <CodeRow
                      hit={hit}
                      highlightTheme={data?.highlightTheme ?? null}
                    />;
                  }
                }
                @if (type === "users") {
                  @for (const hit of usersG?.hits ?? []; key hit.username) {
                    <UserRow hit={hit} />;
                  }
                }
                @if (type === "organizations") {
                  @for (const hit of orgsG?.hits ?? []; key hit.slug) {
                    <OrgRow hit={hit} />;
                  }
                }
              </List>
              @if (hasPrev || hasNext) {
                <div
                  className="mt-6 flex items-center justify-between"
                  data-testid="search-pager"
                >
                  @if (hasPrev) {
                    <AppLink
                      href={
                        tabHref(
                          q0,
                          type,
                          Math.max(0, offset - TAB_LIMIT),
                        ) as string
                      }
                      className="text-[14px] text-primary no-underline hover:underline"
                    >
                      {"← Newer" as string}
                    </AppLink>;
                  } @else {
                    <span />;
                  }
                  @if (hasNext) {
                    <AppLink
                      href={tabHref(q0, type, offset + TAB_LIMIT) as string}
                      className="text-[14px] text-primary no-underline hover:underline"
                    >
                      {"Older →" as string}
                    </AppLink>;
                  }
                </div>;
              }
            </>;
          }
        </>;
      }
    </div>
  </div>;
}
