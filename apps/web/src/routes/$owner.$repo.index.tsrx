import { queryOptions, useQuery } from "@octanejs/tanstack-query";
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
import { FileTree } from "@/components/repo/file-tree";
import { CloneBox } from "@/components/repo/clone-box";
import { QuickSetup } from "@/components/repo/quick-setup";
import { ReadmePanel } from "@/components/repo/readme-panel";
import { RepoAboutSidebar } from "@/components/repo/repo-about-sidebar";
import { RepoBrowseToolbar } from "@/components/repo/repo-browse-toolbar";
import { RepoLastCommitBar } from "@/components/repo/repo-last-commit-bar";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Skeleton } from "@/components/ui/skeleton";
import { findReadmeName, resolveAboutRootFiles, shortRefName } from "@/lib/repo-browse";
import { renderGfm } from "@/lib/markdown";
import { AppPage } from "@/components/app-page";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { usePathname } from "@/lib/pathname.tsrx";
import { repoParamsFromPathname } from "@/lib/route-params";
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

export type CodeHomeLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "empty";
      repo: RepoPublic;
      latestRelease: ReleasePublic | null;
      packages: PackagePublic[];
      contributors: RepoContributorPublic[];
      languages: RepoLanguageStat[];
      licenseFile: string | null;
      licenseLabel: string | null;
      contributingFile: string | null;
    }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      entries: RepoTreeEntry[];
      refs: RepoRefEntry[];
      /** Pre-rendered sanitized HTML (no client markdown skeleton). */
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

async function loadAboutExtras(
  owner: string,
  repoName: string,
  repoId: string,
  fetchReleases: typeof fetchReleaseList,
  fetchPackages: typeof fetchPackagesForRepo,
  fetchContributors: typeof fetchRepoContributors,
  fetchLanguages: typeof fetchRepoLanguages,
): Promise<{
  latestRelease: ReleasePublic | null;
  packages: PackagePublic[];
  contributors: RepoContributorPublic[];
  languages: RepoLanguageStat[];
}> {
  const [relRes, pkgRes, contribRes, langRes] = await Promise.all([
    fetchReleases({ owner, name: repoName }).catch(() => null),
    fetchPackages({ repository_id: repoId }).catch(() => null),
    fetchContributors({ owner, name: repoName, limit: 30 }).catch(() => null),
    fetchLanguages({ owner, name: repoName }).catch(() => null),
  ]);
  const latestRelease =
    relRes && relRes.ok && relRes.data.releases.length > 0 ? relRes.data.releases[0] : null;
  const packages = pkgRes && pkgRes.ok ? pkgRes.data.packages : [];
  const contributors = contribRes && contribRes.ok ? contribRes.data.contributors : [];
  const languages = langRes && langRes.ok ? langRes.data.languages : [];
  return { latestRelease, packages, contributors, languages };
}

async function loadCodeHome(
  owner: string,
  repoName: string,
  fetchGet: typeof fetchRepoGet,
  fetchTree: typeof fetchRepoTree,
  fetchRefs: typeof fetchRepoRefs,
  fetchBlob: typeof fetchRepoBlob,
  fetchCommits: typeof fetchRepoCommits,
  fetchPathLast: typeof fetchRepoPathLastCommits,
  fetchCount: typeof fetchRepoCommitCount,
  fetchReleases: typeof fetchReleaseList,
  fetchPackages: typeof fetchPackagesForRepo,
  fetchContributors: typeof fetchRepoContributors,
  fetchLanguages: typeof fetchRepoLanguages = fetchRepoLanguages,
): Promise<CodeHomeLoaderData> {
  const got = await fetchGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const repo = got.data;
  const refName = repo.default_branch || "main";
  const aboutP = loadAboutExtras(
    owner,
    repoName,
    repo.id,
    fetchReleases,
    fetchPackages,
    fetchContributors,
    fetchLanguages,
  );
  const [treeRes, refsRes] = await Promise.all([
    fetchTree({ owner, name: repoName, ref: refName, path: "" }),
    fetchRefs({ owner, name: repoName }),
  ]);
  if (!treeRes.ok) {
    if (treeRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: treeRes.error.message || "Could not load tree.",
    };
  }
  const refs = refsRes.ok ? refsRes.data.refs : [];
  const about = await aboutP;
  if (treeRes.data.empty) {
    return {
      kind: "empty",
      repo,
      ...about,
      licenseFile: null,
      licenseLabel: null,
      contributingFile: null,
    };
  }
  const entries = treeRes.data.entries;
  const shortRef = shortRefName(treeRes.data.ref) || refName;

  async function fetchUtf8Blob(path: string): Promise<string | null> {
    const blobRes = await fetchBlob({ owner, name: repoName, ref: shortRef, path });
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

  const [commitsRes, pathRes, countRes, readme, aboutFiles] = await Promise.all([
    fetchCommits({ owner, name: repoName, ref: shortRef, skip: 0, limit: 1 }).catch(() => null),
    fetchPathLast({ owner, name: repoName, ref: shortRef, path: "" }).catch(() => null),
    fetchCount({ owner, name: repoName, ref: shortRef }).catch(() => null),
    (async () => {
      const readmeName = findReadmeName(entries);
      if (!readmeName) return null;
      const content = await fetchUtf8Blob(readmeName);
      if (!content) return null;
      return {
        name: readmeName,
        html: await renderGfm(content, {
          owner,
          repo: repoName,
        }),
      };
    })(),
    resolveAboutRootFiles(entries, fetchUtf8Blob),
  ]);

  const tip =
    commitsRes && commitsRes.ok && commitsRes.data.commits.length > 0
      ? commitsRes.data.commits[0]
      : null;
  const commitCount = countRes && countRes.ok ? Number(countRes.data.count) : null;
  const lastCommits =
    pathRes && pathRes.ok ? (pathRes.data.commits as Record<string, RepoCommitSummary>) : {};

  return {
    kind: "ready",
    repo,
    refName: shortRef,
    entries,
    refs,
    readme,
    tip,
    commitCount,
    lastCommits,
    ...about,
    licenseFile: aboutFiles.licenseFile,
    licenseLabel: aboutFiles.licenseLabel,
    contributingFile: aboutFiles.contributingFile,
  };
}

/**
 * Code-home data query — same fetch plan as the old route loader, now a
 * TanStack Query. Title/atom head tags moved to the middleware meta map.
 */
function codeHomeQueryOptions(owner: string, repoName: string) {
  return queryOptions({
    queryKey: ["repo", "code-home", owner, repoName],
    queryFn: async (): Promise<CodeHomeLoaderData> => {
      try {
        return await loadCodeHome(
          owner,
          repoName,
          fetchRepoGet,
          fetchRepoTree,
          fetchRepoRefs,
          fetchRepoBlob,
          fetchRepoCommits,
          fetchRepoPathLastCommits,
          fetchRepoCommitCount,
          fetchReleaseList,
          fetchPackagesForRepo,
          fetchRepoContributors,
          fetchRepoLanguages,
        );
      } catch {
        return {
          kind: "error",
          message: "Can't reach Oxidean. Check your connection and try again.",
        };
      }
    },
    retry: false,
    staleTime: 30_000,
  });
}

/** Code home — Quick setup when empty; tree + README + About when populated (issue #23). */
export function RepoCodeHome() @{
  const pathname = usePathname();
  const params = repoParamsFromPathname(pathname) ?? { owner: "", repo: "" };
  const owner = params.owner;
  const repoName = params.repo;
  const q = useQuery({
    ...codeHomeQueryOptions(owner, repoName),
    enabled: Boolean(owner && repoName),
  });
  const phase: { kind: "loading" } | CodeHomeLoaderData = q.data ?? {
    kind: "loading",
  };

  // Chrome comes from `RepoLayout` (`$owner.$repo.tsrx`) — D-QH-01.
  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else {
    <>
      @if (phase.kind === "loading") {
        <div className="flex flex-col gap-4" aria-busy="true">
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
          @if (phase.kind === "empty") {
            <div className="grid grid-cols-1 gap-8 lg:grid-cols-[minmax(0,1fr)_280px]">
              <div>
                <div className="mb-4 flex flex-wrap items-center justify-end gap-3">
                  <CloneBox
                    owner={owner}
                    repo={repoName}
                    refName={phase.repo.default_branch || "main"}
                    empty={true}
                  />
                </div>
                <QuickSetup owner={owner} repo={repoName} />
              </div>
              <RepoAboutSidebar
                repo={phase.repo}
                latestRelease={phase.latestRelease}
                packages={phase.packages}
                contributors={phase.contributors}
                languages={phase.languages}
                hasReadme={false}
                showActivity={true}
              />
            </div>;
          } @else {
            <div className="grid grid-cols-1 gap-8 lg:grid-cols-[minmax(0,1fr)_280px]">
              <div className="min-w-0">
                <RepoBrowseToolbar
                  owner={owner}
                  repo={repoName}
                  refName={phase.refName}
                  refs={phase.refs}
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
                    basePath=""
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
    </>;
  }
}

/** `/{owner}/{repo}` page root — RepoLayout (chrome + store) around code home. */
export function RepoIndexPage() @{
  <AppPage>
    <RepoLayout>
      <RepoCodeHome />
    </RepoLayout>
  </AppPage>;
}
