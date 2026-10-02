import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
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
import { CloneBox } from "@/components/repo/clone-box";
import { QuickSetup } from "@/components/repo/quick-setup";
import { ReadmePanel } from "@/components/repo/readme-panel";
import { RepoAboutSidebar } from "@/components/repo/repo-about-sidebar";
import { RepoBrowseToolbar } from "@/components/repo/repo-browse-toolbar";
import { RepoLastCommitBar } from "@/components/repo/repo-last-commit-bar";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Skeleton } from "@/components/ui/skeleton";
import { apiClient } from "@/lib/api-client";
import { findReadmeName, resolveAboutRootFiles, shortRefName } from "@/lib/repo-browse";
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
    fetchReleases({ data: { owner, name: repoName } }).catch(() => null),
    fetchPackages({ data: { repository_id: repoId } }).catch(() => null),
    fetchContributors({ data: { owner, name: repoName, limit: 30 } }).catch(() => null),
    fetchLanguages({ data: { owner, name: repoName } }).catch(() => null),
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
  const got = await fetchGet({ data: { owner, name: repoName } });
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
    fetchTree({ data: { owner, name: repoName, ref: refName, path: "" } }),
    fetchRefs({ data: { owner, name: repoName } }),
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
    const blobRes = await fetchBlob({
      data: { owner, name: repoName, ref: shortRef, path },
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

  const [commitsRes, pathRes, countRes, readme, aboutFiles] = await Promise.all([
    fetchCommits({ data: { owner, name: repoName, ref: shortRef, skip: 0, limit: 1 } }).catch(
      () => null,
    ),
    fetchPathLast({ data: { owner, name: repoName, ref: shortRef, path: "" } }).catch(() => null),
    fetchCount({ data: { owner, name: repoName, ref: shortRef } }).catch(() => null),
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

export const Route = createFileRoute("/$owner/$repo/")({
  component: RepoCodeHome,
  head: ({ params }) => ({
    meta: [
      {
        title: `${params.owner}/${params.repo} · Oxidean`,
      },
    ],
    links: [
      {
        rel: "alternate",
        type: "application/atom+xml",
        title: `${params.owner}/${params.repo} activity feed`,
        href: `/api/repos/${params.owner}/${params.repo}/activity.atom`,
      },
    ],
  }),
  loader: async ({ params }): Promise<CodeHomeLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
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
});

type BrowsePhase = { kind: "loading" } | CodeHomeLoaderData;

/** Code home — Quick setup when empty; tree + README + About when populated (issue #23). */
export function RepoCodeHome() @{
  const params = useParams({ from: "/$owner/$repo/" }) as {
    owner: string;
    repo: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const loaderData = useLoaderData({ from: "/$owner/$repo/" }) as CodeHomeLoaderData | undefined;

  const [phase, setPhase] = useState<BrowsePhase>(() =>
    loaderData ? loaderData : { kind: "loading" },
  );

  // Client fallback for unit tests / soft navigations without loader payload.
  useEffect(() => {
    if (loaderData) {
      setPhase(loaderData);
      return;
    }
    let cancelled = false;
    void (async () => {
      setPhase({ kind: "loading" });
      try {
        const next = await loadCodeHome(
          owner,
          repoName,
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
        if (!cancelled) {
          setPhase({
            kind: "error",
            message: "Can't reach Oxidean. Check your connection and try again.",
          });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, loaderData]);

  // Chrome comes from `$owner.$repo` layout (D-QH-01).
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
