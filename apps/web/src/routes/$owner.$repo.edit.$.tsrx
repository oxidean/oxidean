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
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { apiClient } from "@/lib/api-client";
import { blobHref, parseRefAndPath, shortRefName } from "@/lib/repo-browse";
import { fetchRepoBlob, fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";

type EditLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      path: string;
      /** UTF-8 content ("" for binary/truncated — the editor refuses those). */
      content: string;
      editable: boolean;
    };

type Phase = { kind: "loading" } | EditLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadEditPage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<EditLoaderData> {
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
    if (
      blobRes.error.code === "repo.not_found" ||
      blobRes.error.code === "git.not_found" ||
      blobRes.error.code === "repo.git_failed"
    ) {
      return { kind: "not_found" };
    }
    return {
      kind: "error",
      message: blobRes.error.message || "Could not load file.",
    };
  }
  const blob: RepoBlobResponse = blobRes.data;
  const editable = !blob.is_binary && !blob.truncated && blob.encoding === "utf-8";
  return {
    kind: "ready",
    repo,
    refName: shortRefName(blob.ref) || refName,
    path: blob.path || parsed.path,
    content: editable && blob.content ? blob.content : "",
    editable,
  };
}

function repoEditQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/edit/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<EditLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadEditPage(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Edit (and optionally rename/move) a text file — GIT-19. */
function RepoFileEditPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/edit/$", pathname) ?? {}) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const splat = params._splat;
  const loaderData = useQuery(repoEditQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() => loaderData ?? { kind: "loading" });
  const [path, setPath] = useState("");
  const [content, setContent] = useState("");
  const [origContent, setOrigContent] = useState("");
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
        const next = await loadEditPage(owner, repoName, splat);
        if (!cancelled) setPhase(next);
      } catch {
        if (!cancelled) setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, splat, loaderData]);

  // (Re)seed editor state when a fresh blob lands.
  useEffect(() => {
    if (phase.kind === "ready") {
      setPath(phase.path);
      setContent(phase.content);
      setOrigContent(phase.content);
    }
  }, [phase]);

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
    // Policy is advisory — on failure the server still applies the
    // protected-branch fallback on submit.
    retry: 0,
  });

  async function submit(values: FileCommitValues) {
    if (!ready) return;
    setError("");
    const nextPath = path.trim().replace(/^\/+|\/+$/g, "");
    if (!nextPath) {
      setError("File path is required.");
      return;
    }
    const target = {
      branch: refName,
      new_branch: values.mode === "branch" ? values.new_branch.trim() || undefined : undefined,
      pr_title: values.mode === "branch" ? values.pr_title.trim() || undefined : undefined,
    };
    setPending(true);
    try {
      const res =
        nextPath !== ready.path
          ? await apiClient.repo.file.rename({
              owner,
              name: repoName,
              from_path: ready.path,
              to_path: nextPath,
              content: content !== origContent ? content : undefined,
              message: values.message.trim(),
              ...target,
            })
          : await apiClient.repo.file.update({
              owner,
              name: repoName,
              path: nextPath,
              content,
              message: values.message.trim(),
              ...target,
            });
      if (!res.ok) {
        setError(res.error.message || "Could not commit changes.");
        setPending(false);
        return;
      }
      if (res.data.pr_number) {
        window.location.assign(`/${owner}/${repoName}/pull/${res.data.pr_number}`);
      } else {
        window.location.assign(blobHref(owner, repoName, res.data.branch, nextPath));
      }
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <p className="text-[16px] text-muted-foreground">Loading file…</p>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else if (!canWrite) {
    <p role="alert" className="text-[16px] text-destructive">
      You need write access to edit files.
    </p>;
  } @else {
    <>
      <div className="mb-4">
        <PathBreadcrumb owner={owner} repo={repoName} refName={refName} path={phase.path} />
      </div>
      @if (!phase.editable) {
        <div
          role="status"
          className="mb-4 rounded-md border border-border bg-muted/30 px-4 py-3"
        >
          <p className="text-[14px] text-muted-foreground">
            Binary or oversized files can't be edited in the browser. You can still rename or
            delete the file, or upload a replacement.
          </p>
        </div>;
      }
      <div className="mb-4 overflow-hidden rounded-md border border-border bg-card">
        <div className="border-b border-border bg-muted/35 px-3 py-2">
          <Input
            value={path}
            onInput={(e) => setPath((e.target as HTMLInputElement).value)}
            disabled={pending}
            aria-label="File path"
            title="File path — edit to rename or move"
            className="max-w-md font-mono text-[13px]"
            data-testid="file-edit-path"
          />
        </div>
        <Textarea
          value={content}
          onInput={(e) => setContent((e.target as HTMLTextAreaElement).value)}
          disabled={pending || !phase.editable}
          aria-label="File contents"
          className="min-h-[24rem] resize-y rounded-none border-0 font-mono text-[13px] leading-5 focus-visible:ring-0"
          data-testid="file-edit-content"
          spellCheck={false}
        />
      </div>
      <FileCommitForm
        policy={policyQ.data ?? null}
        policyPending={policyQ.isPending && canWrite}
        baseBranch={refName}
        pending={pending}
        error={error}
        submitLabel="Commit changes"
        messagePlaceholder={`Update ${phase.path}`}
        cancelHref={blobHref(owner, repoName, refName, phase.path) as string}
        onSubmit={submit}
      />
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoFileEditPage() @{
  <AppPage>
    <RepoLayout>
      <RepoFileEditPageContent />
    </RepoLayout>
  </AppPage>;
}
