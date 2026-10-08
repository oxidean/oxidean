import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type {
  PackagePublic,
  ReleasePublic,
  RepoCommitSummary,
  RepoContributorPublic,
  RepoLanguageStat,
  RepoPublic,
  RepoRefEntry,
  RepoTreeEntry,
} from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { FileTree } from "@/components/repo/file-tree";
import { RepoAboutSidebar } from "@/components/repo/repo-about-sidebar";
import { RepoBrowseToolbar } from "@/components/repo/repo-browse-toolbar";
import { RepoLastCommitBar } from "@/components/repo/repo-last-commit-bar";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { ReadmePanel } from "@/components/repo/readme-panel";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import {
  findReadmeName,
  joinRepoPath,
  parseRefAndPath,
  resolveAboutRootFiles,
  shortRefName,
} from "@/lib/repo-browse";
import { renderGfm } from "@/lib/markdown";
import {
  fetchPackagesForRepo,
  fetchReleaseList,
  fetchRepoBlob,
  fetchRepoCommitCount,
  fetchRepoCommits,
  fetchRepoContributors,
  fetchRepoGet,
  fetchRepoLanguages,
  fetchRepoPathLastCommits,
  fetchRepoRefs,
  fetchRepoTree,
} from "@/lib/ssr-repo";

export type TreeLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      path: string;
      entries: RepoTreeEntry[];
      refs: RepoRefEntry[];
      /** Nested-dir README when present (same rules as code home). */
      readme: { name: string; html: string } | null;
      tip: RepoCommitSummary | null;
      commitCount: number | null;
      lastCommits: Record<string, RepoCommitSummary>;
      latestRelease: ReleasePublic | null;
      packages: PackagePublic[];
      contributors: RepoContributorPublic[];
      languages: RepoLanguageStat[];
      licenseFile: string | null;
      licenseLabel: string | null;
      contributingFile: string | null;
    };

type Phase = { kind: "loading" } | TreeLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadTreePage(
  owner: string,
  repoName: string,
  splat: string | undefined,
  fetchGet: typeof fetchRepoGet = fetchRepoGet,
  fetchTree: typeof fetchRepoTree = fetchRepoTree,
  fetchRefs: typeof fetchRepoRefs = fetchRepoRefs,
  fetchBlob: typeof fetchRepoBlob = fetchRepoBlob,
  fetchCommits: typeof fetchRepoCommits = fetchRepoCommits,
  fetchPathLast: typeof fetchRepoPathLastCommits = fetchRepoPathLastCommits,
  fetchCount: typeof fetchRepoCommitCount = fetchRepoCommitCount,
  fetchReleases: typeof fetchReleaseList = fetchReleaseList,
  fetchPackages: typeof fetchPackagesForRepo = fetchPackagesForRepo,
  fetchContributors: typeof fetchRepoContributors = fetchRepoContributors,
  fetchLanguages: typeof fetchRepoLanguages = fetchRepoLanguages,
): Promise<TreeLoaderData> {
  const got = await fetchGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const repo = got.data;
  const refsRes = await fetchRefs({ owner, name: repoName });
  const refs = refsRes.ok ? refsRes.data.refs : [];
  const knownRefs = refs.map((r) => shortRefName(r.name));
  const parsed = parseRefAndPath(splat, knownRefs);
  const refName = parsed.ref || repo.default_branch || "main";
  const path = parsed.path || "";
  const treeRes = await fetchTree({
    owner,
    name: repoName,
    ref: refName,
    path,
  });
  if (!treeRes.ok) {
    if (treeRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: treeRes.error.message || "Could not load tree.",
    };
  }
  const entries = treeRes.data.entries;
  const dirPath = treeRes.data.path || path;
  const shortRef = shortRefName(treeRes.data.ref) || refName;
  const atRoot = !dirPath;

  async function fetchUtf8Blob(relPath: string): Promise<string | null> {
    const blobRes = await fetchBlob({
      owner,
      name: repoName,
      ref: shortRef,
      path: relPath,
    });
    if (
      blobRes.ok &&
      !blobRes.data.is_binary &&
      blobRes.data.encoding === "utf-8" &&
      blobRes.data.content
    ) {
      return blobRes.data.content;
    }
    return null;
  }

  const [commitsRes, pathRes, countRes, relRes, pkgRes, contribRes, langRes, readme, aboutFiles] =
    await Promise.all([
      fetchCommits({ owner, name: repoName, ref: shortRef, skip: 0, limit: 1 }).catch(() => null),
      fetchPathLast({ owner, name: repoName, ref: shortRef, path: dirPath }).catch(() => null),
      fetchCount({ owner, name: repoName, ref: shortRef }).catch(() => null),
      fetchReleases({ owner, name: repoName }).catch(() => null),
      fetchPackages({ repository_id: repo.id }).catch(() => null),
      fetchContributors({ owner, name: repoName, limit: 30 }).catch(() => null),
      fetchLanguages({ owner, name: repoName }).catch(() => null),
      (async () => {
        const readmeName = findReadmeName(entries);
        if (!readmeName) return null as { name: string; html: string } | null;
        const content = await fetchUtf8Blob(joinRepoPath(dirPath, readmeName));
        if (!content) return null;
        return {
          name: readmeName,
          html: await renderGfm(content, {
            owner,
            repo: repoName,
          }),
        };
      })(),
      atRoot
        ? resolveAboutRootFiles(entries, fetchUtf8Blob)
        : Promise.resolve({
            licenseFile: null as string | null,
            licenseLabel: null as string | null,
            contributingFile: null as string | null,
          }),
    ]);

  const tip =
    commitsRes && commitsRes.ok && commitsRes.data.commits.length > 0
      ? commitsRes.data.commits[0]
      : null;
  // For nested dirs, tip of whole ref is less ideal — still show branch tip for parity.
  const commitCount = countRes && countRes.ok ? Number(countRes.data.count) : null;
  const lastCommits =
    pathRes && pathRes.ok ? (pathRes.data.commits as Record<string, RepoCommitSummary>) : {};
  const latestRelease =
    relRes && relRes.ok && relRes.data.releases.length > 0 ? relRes.data.releases[0] : null;
  const packages = pkgRes && pkgRes.ok ? pkgRes.data.packages : [];
  const contributors = contribRes && contribRes.ok ? contribRes.data.contributors : [];
  const languages = langRes && langRes.ok ? langRes.data.languages : [];

  return {
    kind: "ready",
    repo,
    refName: shortRef,
    path: dirPath,
    entries,
    refs,
    readme,
    tip,
    commitCount,
    lastCommits,
    latestRelease,
    packages,
    contributors,
    languages,
    licenseFile: aboutFiles.licenseFile,
    licenseLabel: aboutFiles.licenseLabel,
    contributingFile: aboutFiles.contributingFile,
  };
}

function repoTreeQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/tree/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<TreeLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadTreePage(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Nested tree — toolbar / last-commit / file list / About / README (issue #23). */
function RepoTreePageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/tree/$", pathname) ?? {}) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const splat = params._splat;
  const loaderData = useQuery(repoTreeQueryOptions(params, searchStr)).data;

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
        const next = await loadTreePage(
          owner,
          repoName,
          splat,
          async ({ data }) => apiClient.repo.get(data) as never,
          async ({ data }) => apiClient.repo.tree(data) as never,
          async ({ data }) => apiClient.repo.refs(data) as never,
          async ({ data }) => apiClient.repo.blob(data) as never,
          async ({ data }) => apiClient.repo.commits(data) as never,
          async ({ data }) => apiClient.repo.pathLastCommits(data) as never,
          async ({ data }) => apiClient.repo.commitCount(data) as never,
          async ({ data }) => apiClient.release.list(data) as never,
          async ({ data }) => apiClient.packages.list(data) as never,
          async ({ data }) => apiClient.repo.contributorsList(data) as never,
          async ({ data }) => apiClient.repo.languages(data) as never,
        );
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
  } @else {
    @if (phase.kind === "loading") {
      <div className="flex flex-col gap-4" aria-busy="true" data-testid="repo-tree-loading">
        <Skeleton className="h-8 w-64" />
        <Skeleton className="h-10 w-full" />
        <Skeleton className="h-48 w-full" />
      </div>;
    } @else {
      @if (phase.kind === "error") {
        <p role="alert" className="text-[16px] text-destructive">
          {phase.message as string}
        </p>;
      } @else {
        <div className="grid grid-cols-1 gap-8 lg:grid-cols-[minmax(0,1fr)_280px]">
          <div className="min-w-0">
            <RepoBrowseToolbar
              owner={owner}
              repo={repoName}
              refName={phase.refName}
              refs={phase.refs}
              path={phase.path}
              showPathCrumbs={true}
              empty={false}
              canWrite={!!phase.repo.can_write}
            />
            <div className="mt-4">
              <RepoLastCommitBar
                owner={owner}
                repo={repoName}
                refName={phase.refName}
                tip={phase.tip}
                commitCount={phase.commitCount}
              />
              <FileTree
                owner={owner}
                repo={repoName}
                refName={phase.refName}
                basePath={phase.path}
                entries={phase.entries}
                lastCommits={phase.lastCommits}
              />
            </div>
            @if (phase.readme) {
              <ReadmePanel html={phase.readme.html} fileName={phase.readme.name} />;
            }
          </div>
          <RepoAboutSidebar
            repo={phase.repo}
            latestRelease={phase.latestRelease}
            packages={phase.packages}
            contributors={phase.contributors}
            languages={phase.languages}
            hasReadme={!!phase.readme}
            refName={phase.refName}
            license={
              phase.licenseFile
                ? { path: phase.licenseFile, label: phase.licenseLabel || "License" }
                : null
            }
            contributingPath={phase.contributingFile}
            showActivity={true}
          />
        </div>;
      }
    }
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoTreePage() @{
  <AppPage>
    <RepoLayout>
      <RepoTreePageContent />
    </RepoLayout>
  </AppPage>;
}
