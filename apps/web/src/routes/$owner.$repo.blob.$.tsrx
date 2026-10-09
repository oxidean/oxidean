import type { RepoBlobResponse, RepoPublic, RepoRefEntry } from "@oxidean/api-client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useEffect, useState } from "octane";
import { BlobViewer } from "@/components/repo/blob-viewer";
import { PathBreadcrumb } from "@/components/repo/path-breadcrumb";
import { RefSelect } from "@/components/repo/ref-select";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { highlightCode, languageIdForPath, type HighlightTheme } from "@/lib/highlight";
import { isMarkdownPath, renderGfm } from "@/lib/markdown";
import { parseRefAndPath, shortRefName } from "@/lib/repo-browse";
import { resolveDocumentHighlightTheme } from "@/lib/theme";
import { fetchRepoBlob, fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";

type BlobLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      path: string;
      blob: RepoBlobResponse;
      refs: RepoRefEntry[];
      /** Pre-rendered Shiki HTML (SSR). */
      highlightedHtml: string | null;
      highlightTheme: HighlightTheme | null;
      /** Pre-rendered GFM HTML for markdown paths. */
      markdownHtml: string | null;
    };

type Phase = { kind: "loading" } | BlobLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function highlightBlob(
  path: string,
  content: string,
): Promise<{ html: string; theme: HighlightTheme }> {
  const theme = resolveDocumentHighlightTheme();
  const html = await highlightCode(content, {
    lang: languageIdForPath(path),
    theme,
  });
  return { html, theme };
}

async function loadBlobPage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<BlobLoaderData> {
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
  const knownRefs = refs.map((r) => shortRefName(r.name));
  const parsed = parseRefAndPath(splat, knownRefs);
  const refName = parsed.ref || repo.default_branch || "main";
  if (!parsed.path) {
    return { kind: "not_found" };
  }
  const blobRes = await fetchRepoBlob({ owner, name: repoName, ref: refName, path: parsed.path });
  if (!blobRes.ok) {
    if (blobRes.error.code === "repo.not_found" || blobRes.error.code === "git.not_found") {
      return { kind: "not_found" };
    }
    return {
      kind: "error",
      message: blobRes.error.message || "Could not load file.",
    };
  }

  let highlightedHtml: string | null = null;
  let highlightTheme: HighlightTheme | null = null;
  let markdownHtml: string | null = null;
  const blob = blobRes.data;
  const pathForRender = blob.path || parsed.path;
  if (!blob.is_binary && !blob.truncated && blob.encoding === "utf-8" && blob.content) {
    if (isMarkdownPath(pathForRender)) {
      try {
        markdownHtml = await renderGfm(blob.content, {
          owner,
          repo: repoName,
        });
      } catch {
        markdownHtml = null;
      }
    }
    try {
      const hl = await highlightBlob(pathForRender, blob.content);
      highlightedHtml = hl.html;
      highlightTheme = hl.theme;
    } catch {
      highlightedHtml = null;
      highlightTheme = null;
    }
  }

  return {
    kind: "ready",
    repo,
    refName: shortRefName(blob.ref) || refName,
    path: pathForRender,
    blob,
    refs,
    highlightedHtml,
    highlightTheme,
    markdownHtml,
  };
}

function repoBlobQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/blob/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<BlobLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadBlobPage(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function RepoBlobPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/blob/$", pathname) ?? {}) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const splat = params._splat;
  const loaderData = useQuery(repoBlobQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() => loaderData ?? { kind: "loading" });

  useEffect(() => {
    if (loaderData) {
      setPhase(loaderData);
      return;
    }
    let cancelled = false;
    void (async () => {
      setPhase({ kind: "loading" });
      try {
        const next = await loadBlobPage(owner, repoName, splat);
        if (!cancelled) setPhase(next);
      } catch {
        if (!cancelled) setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, splat, loaderData]);

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <p className="text-[16px] text-muted-foreground">Loading file…</p>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else {
    <>
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <RefSelect
          owner={owner}
          repo={repoName}
          refs={phase.refs}
          value={phase.refName}
          pathUnderRef={phase.path}
          mode="blob"
        />
        <PathBreadcrumb owner={owner} repo={repoName} refName={phase.refName} path={phase.path} />
      </div>
      <BlobViewer
        owner={owner}
        repo={repoName}
        blob={phase.blob}
        canWrite={!!phase.repo.can_write}
        highlightedHtml={phase.highlightedHtml}
        highlightTheme={phase.highlightTheme}
        markdownHtml={phase.markdownHtml}
      />
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoBlobPage() @{
  <AppPage>
    <RepoLayout>
      <RepoBlobPageContent />
    </RepoLayout>
  </AppPage>;
}
