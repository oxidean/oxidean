import { navigate } from "astro:transitions/client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoPublic, RepoRefEntry } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { FileCommitForm, type FileCommitValues } from "@/components/repo/file-commit-form";
import { PathBreadcrumb } from "@/components/repo/path-breadcrumb";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { blobHref, joinRepoPath, parseRefAndPath, shortRefName, treeHref } from "@/lib/repo-browse";
import { fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";

type NewFileLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      /** Directory the new file lands in ("" = repo root). */
      dir: string;
    };

type Phase = { kind: "loading" } | NewFileLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadNewFilePage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<NewFileLoaderData> {
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
  return {
    kind: "ready",
    repo,
    refName,
    dir: parsed.path,
  };
}

function repoNewQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/new/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<NewFileLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadNewFilePage(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Create a new text file in a directory — GIT-19. */
function RepoFileNewPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/new/$", pathname) ?? {}) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const splat = params._splat;
  const loaderData = useQuery(repoNewQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() => loaderData ?? { kind: "loading" });
  const [name, setName] = useState("");
  const [content, setContent] = useState("");
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
        const next = await loadNewFilePage(owner, repoName, splat);
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
    const fileName = name.trim().replace(/^\/+|\/+$/g, "");
    if (!fileName) {
      setError("File name is required.");
      return;
    }
    const fullPath = joinRepoPath(dir, fileName);
    setPending(true);
    try {
      const res = await apiClient.repo.file.create({
        owner,
        name: repoName,
        path: fullPath,
        content,
        message: values.message.trim(),
        branch: refName,
        new_branch: values.mode === "branch" ? values.new_branch.trim() || undefined : undefined,
        pr_title: values.mode === "branch" ? values.pr_title.trim() || undefined : undefined,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not create the file.");
        setPending(false);
        return;
      }
      if (res.data.pr_number) {
        void navigate(`/${owner}/${repoName}/pull/${res.data.pr_number}`);
      } else {
        void navigate(blobHref(owner, repoName, res.data.branch, fullPath));
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
      You need write access to create files.
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
          aria-label="File name"
          placeholder="name.md"
          className="max-w-xs font-mono text-[13px]"
          data-testid="file-new-name"
        />
      </div>
      <div className="mb-4 overflow-hidden rounded-md border border-border bg-card">
        <Textarea
          value={content}
          onInput={(e) => setContent((e.target as HTMLTextAreaElement).value)}
          disabled={pending}
          aria-label="File contents"
          placeholder="Enter file contents here"
          className="min-h-[24rem] resize-y rounded-none border-0 font-mono text-[13px] leading-5 focus-visible:ring-0"
          data-testid="file-new-content"
          spellCheck={false}
        />
      </div>
      <FileCommitForm
        policy={policyQ.data ?? null}
        policyPending={policyQ.isPending && canWrite}
        baseBranch={refName}
        pending={pending}
        error={error}
        submitLabel="Commit new file"
        messagePlaceholder={name.trim() ? `Create ${name.trim()}` : "Create new file"}
        cancelHref={treeHref(owner, repoName, refName, dir) as string}
        onSubmit={submit}
      />
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoFileNewPage() @{
  <AppPage>
    <RepoLayout>
      <RepoFileNewPageContent />
    </RepoLayout>
  </AppPage>;
}
