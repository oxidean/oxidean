import type { RepoCompareResponse, RepoPublic, RepoRefEntry } from "@oxidean/api-client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useEffect, useState } from "octane";
import { GitCompareArrows } from "@octanejs/lucide";
import { DiffPatch } from "@/components/repo/diff-patch";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import type { HighlightTheme } from "@/lib/highlight";
import type { HighlightedDiffLine } from "@/lib/highlight-diff";
import { compareHref, shortRefName } from "@/lib/repo-browse";
import { ssrHighlightDiffFiles } from "@/lib/ssr-diff-highlight";
import { fetchRepoCompare, fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type CompareLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refs: RepoRefEntry[];
      base: string;
      head: string;
      compare: RepoCompareResponse | null;
      /** Pre-highlighted diff rows by path (SSR). */
      highlightedByPath: Record<string, HighlightedDiffLine[]>;
      highlightTheme: HighlightTheme | null;
    };

type Phase = { kind: "loading" } | CompareLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function parseCompareSplat(splat: string | undefined): {
  base: string;
  head: string;
} {
  const s = splat ?? "";
  const idx = s.indexOf("...");
  if (idx === -1) {
    return { base: decodeURIComponent(s), head: "" };
  }
  return {
    base: decodeURIComponent(s.slice(0, idx)),
    head: decodeURIComponent(s.slice(idx + 3)),
  };
}

async function loadComparePage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<CompareLoaderData> {
  const parsed = parseCompareSplat(splat);
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const repo = got.data;
  const refsRes = await fetchRepoRefs({ owner, name: repoName });
  const refs = refsRes.ok ? refsRes.data.refs : [];
  const base = parsed.base || repo.default_branch || "main";
  const head = parsed.head || repo.default_branch || "main";

  if (!parsed.head) {
    return {
      kind: "ready",
      repo,
      refs,
      base,
      head,
      compare: null,
      highlightedByPath: {},
      highlightTheme: null,
    };
  }

  const cmp = await fetchRepoCompare({ owner, name: repoName, base, head });
  if (!cmp.ok) {
    if (cmp.error.code === "repo.not_found" || cmp.error.code === "repo.path_not_found") {
      return { kind: "not_found" };
    }
    return {
      kind: "error",
      message: cmp.error.message || "Could not compare refs.",
    };
  }
  const hl = await ssrHighlightDiffFiles(cmp.data.files);
  return {
    kind: "ready",
    repo,
    refs,
    base,
    head,
    compare: cmp.data,
    highlightedByPath: hl.byPath,
    highlightTheme: hl.theme,
  };
}

function repoCompareQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/compare/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<CompareLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadComparePage(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function RepoComparePageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/compare/$", pathname) ?? {}) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const parsed = parseCompareSplat(params._splat);
  const loaderData = useQuery(repoCompareQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() => loaderData ?? { kind: "loading" });
  const [baseDraft, setBaseDraft] = useState(() =>
    loaderData?.kind === "ready" ? loaderData.base : parsed.base,
  );
  const [headDraft, setHeadDraft] = useState(() =>
    loaderData?.kind === "ready" ? loaderData.head : parsed.head,
  );

  useEffect(() => {
    if (loaderData) {
      setPhase(loaderData);
      if (loaderData.kind === "ready") {
        setBaseDraft(loaderData.base);
        setHeadDraft(loaderData.head);
      }
      return;
    }
    let cancelled = false;
    void (async () => {
      setPhase({ kind: "loading" });
      try {
        const next = await loadComparePage(owner, repoName, params._splat);
        if (cancelled) return;
        setPhase(next);
        if (next.kind === "ready") {
          setBaseDraft(next.base);
          setHeadDraft(next.head);
        }
      } catch {
        if (!cancelled) setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, parsed.base, parsed.head, loaderData, params._splat]);

  function goCompare() {
    const b = baseDraft.trim() || "main";
    const h = headDraft.trim() || "main";
    if (typeof window !== "undefined") {
      window.location.href = compareHref(owner, repoName, b, h);
    }
  }

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <>
      <PageHeader icon={<GitCompareArrows aria-hidden size={22} />} title="Compare changes" />
      <Skeleton className="mt-6 h-40 w-full" />
    </>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else {
    <>
      <PageHeader
        icon={<GitCompareArrows aria-hidden size={22} />}
        title="Compare changes"
        description="Choose a base and head ref to see what changed between them."
      />
      <div className="mt-6 mb-6 flex flex-wrap items-end gap-3">
        <label className="flex min-w-0 flex-col gap-1 text-[13px] text-muted-foreground">
          Base
          <Input
            className="max-w-[14rem] truncate font-mono"
            value={baseDraft as string}
            list="oxidean-compare-refs"
            onInput={(e) => {
              setBaseDraft((e.currentTarget as HTMLInputElement).value);
            }}
          />
        </label>
        <span className="pb-2 text-muted-foreground">...</span>
        <label className="flex min-w-0 flex-col gap-1 text-[13px] text-muted-foreground">
          Head
          <Input
            className="max-w-[14rem] truncate font-mono"
            value={headDraft as string}
            list="oxidean-compare-refs"
            onInput={(e) => {
              setHeadDraft((e.currentTarget as HTMLInputElement).value);
            }}
          />
        </label>
        <datalist id="oxidean-compare-refs">
          @for (const r of phase.refs; key r.name) {
            <option value={shortRefName(r.name) as string} />;
          }
        </datalist>
        <Button type="button" onClick={() => goCompare()}>
          Compare
        </Button>
        @if (
          phase.compare &&
          !phase.compare.empty &&
          phase.repo.can_write
        ) {
          <AppLink
            className="no-underline"
            href={
              `/${owner}/${repoName}/pulls/new?base=${encodeURIComponent(phase.base)}&head=${encodeURIComponent(phase.head)}` as string
            }
          >
            <Button type="button" variant="outline">
              Create pull request
            </Button>
          </AppLink>;
        }
      </div>
      @if (!phase.compare) {
        <EmptyState
          icon={<GitCompareArrows aria-hidden size={22} />}
          title="Pick refs to compare"
          description="Enter a base and head ref above, then choose Compare."
        />;
      } @else if (phase.compare.empty) {
        <EmptyState
          icon={<GitCompareArrows aria-hidden size={22} />}
          title="Nothing to compare"
          description="These refs point at the same content — there are no differences."
        />;
      } @else {
        <>
          @if (phase.compare.truncated) {
            <p className="mb-3 text-[14px] text-muted-foreground">
              Diff truncated for size.
            </p>;
          }
          <div className="flex flex-col gap-6">
            @for (const f of phase.compare.files; key f.path) {
              <DiffPatch
                path={f.path as string}
                status={f.status as string}
                patch={f.patch as string}
                initialRows={phase.highlightedByPath[f.path] ?? null}
                highlightTheme={phase.highlightTheme}
              />;
            }
          </div>
        </>;
      }
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoComparePage() @{
  <AppPage>
    <RepoLayout>
      <RepoComparePageContent />
    </RepoLayout>
  </AppPage>;
}
