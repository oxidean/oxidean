import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoBlobResponse, RepoPublic, RepoRefEntry } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { FileCommitForm, type FileCommitValues } from "@/components/repo/file-commit-form";
import { PathBreadcrumb } from "@/components/repo/path-breadcrumb";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { apiClient } from "@/lib/api-client";
import {
  blobHref,
  formatFileSize,
  parentRepoPath,
  parseRefAndPath,
  shortRefName,
  treeHref,
} from "@/lib/repo-browse";
import { fetchRepoBlob, fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";

type DeleteLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      path: string;
      /** File or directory (delete removes every blob under a dir). */
      isDir: boolean;
      sizeLabel: string;
    };

type Phase = { kind: "loading" } | DeleteLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadDeletePage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<DeleteLoaderData> {
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
  const refs: RepoRefEntry[] = refsRes.ok ? refsRes.data.refs : [];
  const knownRefs = refs.map((r) => shortRefName(r.name));
  const parsed = parseRefAndPath(splat, knownRefs);
  const refName = parsed.ref || repo.default_branch || "main";
  if (!parsed.path) return { kind: "not_found" };

  const blobRes = await fetchRepoBlob({ owner, name: repoName, ref: refName, path: parsed.path });
  if (!blobRes.ok) {
    // A directory path resolves as a tree, not a blob — probe the tree.
    const { fetchRepoTree } = await import("@/lib/ssr-repo");
    const treeRes = await fetchRepoTree({ owner, name: repoName, ref: refName, path: parsed.path });
    if (treeRes.ok) {
      return {
        kind: "ready",
        repo,
        refName: shortRefName(treeRes.data.ref) || refName,
        path: treeRes.data.path || parsed.path,
        isDir: true,
        sizeLabel: `${treeRes.data.entries.length} entries`,
      };
    }
    if (blobRes.error.code === "repo.not_found" || blobRes.error.code === "git.not_found") {
      return { kind: "not_found" };
    }
    return {
      kind: "error",
      message: blobRes.error.message || "Could not load path.",
    };
  }
  const blob: RepoBlobResponse = blobRes.data;
  return {
    kind: "ready",
    repo,
    refName: shortRefName(blob.ref) || refName,
    path: blob.path || parsed.path,
    isDir: false,
    sizeLabel: formatFileSize(blob.size),
  };
}

function repoDeleteQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/delete/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<DeleteLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadDeletePage(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Delete a file or directory with a signed web commit — GIT-19. */
function RepoFileDeletePageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/delete/$", pathname) ?? {}) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const splat = params._splat;
  const loaderData = useQuery(repoDeleteQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() => loaderData ?? { kind: "loading" });
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (loaderData) {
      setPhase(loaderData);
      return;
    }
    let cancelled = false;
    void (async () => {
      setPhase({ kind: "loading" });
      try {
        const next = await loadDeletePage(owner, repoName, splat);
        if (!cancelled) setPhase(next);
      } catch {
        if (!cancelled) setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, splat, loaderData]);

  const ready = phase.kind === "ready" ? phase : null;
  const repo = ready?.repo ?? null;
  const refName = ready?.refName ?? "";
  const canWrite = !!repo?.can_write;

  const policyQ = useQuery({
    queryKey: ["repo", "fileCommitPolicy", owner, repoName, refName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.file.commitPolicy({
        owner,
        name: repoName,
        branch: refName,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: canWrite && !!owner && !!repoName && !!refName,
    retry: 0,
  });

  async function submit(values: FileCommitValues) {
    if (!ready) return;
    setError("");
    setPending(true);
    try {
      const res = await apiClient.repo.file.delete({
        owner,
        name: repoName,
        path: ready.path,
        message: values.message.trim(),
        branch: refName,
        new_branch: values.mode === "branch" ? values.new_branch.trim() || undefined : undefined,
        pr_title: values.mode === "branch" ? values.pr_title.trim() || undefined : undefined,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not delete the path.");
        setPending(false);
        return;
      }
      if (res.data.pr_number) {
        window.location.assign(`/${owner}/${repoName}/pull/${res.data.pr_number}`);
      } else {
        window.location.assign(
          treeHref(owner, repoName, res.data.branch, parentRepoPath(ready.path)),
        );
      }
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <p className="text-[16px] text-muted-foreground">Loading…</p>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else if (!canWrite) {
    <p role="alert" className="text-[16px] text-destructive">
      You need write access to delete files.
    </p>;
  } @else {
    <>
      <div className="mb-4">
        <PathBreadcrumb owner={owner} repo={repoName} refName={refName} path={phase.path} />
      </div>
      <div
        role="alert"
        className="mb-4 rounded-md border border-destructive/40 bg-destructive/10 px-4 py-3"
      >
        <p className="text-[14px] font-medium text-foreground">
          {
            (phase.isDir
              ? `Delete the directory ${phase.path} and everything inside it?`
              : `Delete ${phase.path}?`) as string
          }
        </p>
        <p className="mt-1 text-[13px] text-muted-foreground">
          {
            (phase.isDir
              ? `${phase.sizeLabel} will be removed. This commits a deletion — it stays recoverable through git history.`
              : `Size ${phase.sizeLabel}. This commits a deletion — the file stays recoverable through git history.`) as string
          }
        </p>
      </div>
      <FileCommitForm
        policy={policyQ.data ?? null}
        policyPending={policyQ.isPending && canWrite}
        baseBranch={refName}
        pending={pending}
        error={error}
        submitLabel="Commit deletion"
        messagePlaceholder={
          (phase.isDir ? `Delete directory ${phase.path}` : `Delete ${phase.path}`) as string
        }
        cancelHref={blobHref(owner, repoName, refName, phase.path) as string}
        onSubmit={submit}
      />
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoFileDeletePage() @{
  <AppPage>
    <RepoLayout>
      <RepoFileDeletePageContent />
    </RepoLayout>
  </AppPage>;
}
