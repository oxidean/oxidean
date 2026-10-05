import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useQuery } from "@octanejs/tanstack-query";
import type { RepoPublic, RepoRefEntry } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { FileCommitForm, type FileCommitValues } from "@/components/repo/file-commit-form";
import { PathBreadcrumb } from "@/components/repo/path-breadcrumb";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Input } from "@/components/ui/input";
import { apiClient } from "@/lib/api-client";
import { joinRepoPath, parseRefAndPath, shortRefName, treeHref } from "@/lib/repo-browse";
import { fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";

export type MkdirLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      /** Parent directory the new directory lands in ("" = repo root). */
      dir: string;
    };

type Phase = { kind: "loading" } | MkdirLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadMkdirPage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<MkdirLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const repo = got.data;
  const refsRes = await fetchRepoRefs({ data: { owner, name: repoName } });
  const refs: RepoRefEntry[] = refsRes.ok ? refsRes.data.refs : [];
  const knownRefs = refs.map((r) => shortRefName(r.name));
  const parsed = parseRefAndPath(splat, knownRefs);
  const refName = parsed.ref || repo.default_branch || "main";
  return { kind: "ready", repo, refName, dir: parsed.path };
}

export const Route = createFileRoute("/$owner/$repo/mkdir/$")({
  component: RepoFileMkdirPage,
  head: ({ params }) => {
    const owner = (params as { owner: string }).owner;
    const repo = (params as { repo: string }).repo;
    return {
      meta: [{ title: `New directory · ${owner}/${repo} · Oxidean` }],
    };
  },
  loader: async ({ params }): Promise<MkdirLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    const splat = (params as { _splat?: string })._splat;
    try {
      return await loadMkdirPage(owner, repoName, splat);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/**
 * Create a directory — GIT-19. Git does not track empty directories, so the
 * commit materializes `{dir}/.gitkeep` (the usual convention).
 */
export function RepoFileMkdirPage() @{
  const params = useParams({ from: "/$owner/$repo/mkdir/$" }) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const splat = params._splat;
  const loaderData = useLoaderData({ from: "/$owner/$repo/mkdir/$" }) as
    | MkdirLoaderData
    | undefined;

  const [phase, setPhase] = useState<Phase>(() => loaderData ?? { kind: "loading" });
  const [name, setName] = useState("");
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
        const next = await loadMkdirPage(owner, repoName, splat);
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
  const dir = ready?.dir ?? "";
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
    const dirName = name.trim().replace(/^\/+|\/+$/g, "");
    if (!dirName) {
      setError("Directory name is required.");
      return;
    }
    const fullPath = joinRepoPath(dir, dirName);
    setPending(true);
    try {
      const res = await apiClient.repo.file.mkdir({
        owner,
        name: repoName,
        path: fullPath,
        message: values.message.trim(),
        branch: refName,
        new_branch: values.mode === "branch" ? values.new_branch.trim() || undefined : undefined,
        pr_title: values.mode === "branch" ? values.pr_title.trim() || undefined : undefined,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not create the directory.");
        setPending(false);
        return;
      }
      if (res.data.pr_number) {
        window.location.assign(`/${owner}/${repoName}/pull/${res.data.pr_number}`);
      } else {
        window.location.assign(treeHref(owner, repoName, res.data.branch, fullPath));
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
      You need write access to create directories.
    </p>;
  } @else {
    <>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <PathBreadcrumb owner={owner} repo={repoName} refName={refName} path={dir} />
        <span className="text-[16px] text-muted-foreground">/</span>
        <Input
          value={name}
          onInput={(e) => setName((e.target as HTMLInputElement).value)}
          disabled={pending}
          aria-label="Directory name"
          placeholder="new-directory"
          className="max-w-xs font-mono text-[13px]"
          data-testid="file-mkdir-name"
        />
      </div>
      <p className="mb-4 max-w-xl text-[14px] text-muted-foreground">
        Git does not track empty directories — the commit adds a placeholder
        <code className="mx-1 rounded bg-muted px-1 font-mono text-[13px]">.gitkeep</code>
        inside the new directory.
      </p>
      <FileCommitForm
        policy={policyQ.data ?? null}
        policyPending={policyQ.isPending && canWrite}
        baseBranch={refName}
        pending={pending}
        error={error}
        submitLabel="Commit new directory"
        messagePlaceholder={name.trim() ? `Create directory ${name.trim()}` : "Create directory"}
        cancelHref={treeHref(owner, repoName, refName, dir) as string}
        onSubmit={submit}
      />
    </>;
  }
}
