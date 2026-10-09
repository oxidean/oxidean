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
import { FileDropzone } from "@/components/ui/file-dropzone";
import { apiClient } from "@/lib/api-client";
import {
  formatFileSize,
  joinRepoPath,
  parseRefAndPath,
  shortRefName,
  treeHref,
} from "@/lib/repo-browse";
import { fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";

type UploadLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      /** Destination directory ("" = repo root). */
      dir: string;
    };

type Phase = { kind: "loading" } | UploadLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
/** Mirrors FILE_EDIT_MAX_BYTES in crates/oxidean-api/src/repo/files.rs. */
const FILE_MAX_BYTES = 8 * 1024 * 1024;
/** Mirrors UPLOAD_MAX_FILES / UPLOAD_MAX_TOTAL_BYTES server-side. */
const UPLOAD_MAX_FILES = 50;
const UPLOAD_MAX_TOTAL = 32 * 1024 * 1024;

async function loadUploadPage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<UploadLoaderData> {
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
  return { kind: "ready", repo, refName, dir: parsed.path };
}

/** Chunked File→base64 (avoids spread-arg stack limits on multi-MB files). */
async function fileToBase64(file: File): Promise<string> {
  const buf = new Uint8Array(await file.arrayBuffer());
  const CHUNK = 0x8000;
  let bin = "";
  for (let i = 0; i < buf.length; i += CHUNK) {
    bin += String.fromCharCode(...buf.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

function repoUploadQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/upload/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<UploadLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadUploadPage(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Multi-file upload committed as one signed commit — GIT-19. */
function RepoFileUploadPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/upload/$", pathname) ?? {}) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const splat = params._splat;
  const loaderData = useQuery(repoUploadQueryOptions(params, searchStr)).data;

  const [phase, setPhase] = useState<Phase>(() => loaderData ?? { kind: "loading" });
  const [files, setFiles] = useState<File[]>([]);
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
        const next = await loadUploadPage(owner, repoName, splat);
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

  function addFiles(next: File[]) {
    setError("");
    setFiles((prev) => {
      const merged = [...prev];
      for (const f of next) {
        if (merged.some((m) => m.name === f.name && m.size === f.size)) continue;
        merged.push(f);
      }
      return merged.slice(0, UPLOAD_MAX_FILES);
    });
  }

  function removeFile(name: string) {
    setFiles((prev) => prev.filter((f) => f.name !== name));
  }

  async function submit(values: FileCommitValues) {
    if (!ready) return;
    setError("");
    if (files.length === 0) {
      setError("Add at least one file.");
      return;
    }
    const total = files.reduce((acc, f) => acc + f.size, 0);
    if (total > UPLOAD_MAX_TOTAL) {
      setError(`Upload exceeds the ${Math.round(UPLOAD_MAX_TOTAL / 1024 / 1024)} MB limit.`);
      return;
    }
    setPending(true);
    try {
      const entries = await Promise.all(
        files.map(async (f) => ({
          path: joinRepoPath(dir, f.name),
          content_base64: await fileToBase64(f),
        })),
      );
      const res = await apiClient.repo.file.upload({
        owner,
        name: repoName,
        files: entries,
        message: values.message.trim(),
        branch: refName,
        new_branch: values.mode === "branch" ? values.new_branch.trim() || undefined : undefined,
        pr_title: values.mode === "branch" ? values.pr_title.trim() || undefined : undefined,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not upload the files.");
        setPending(false);
        return;
      }
      if (res.data.pr_number) {
        window.location.assign(`/${owner}/${repoName}/pull/${res.data.pr_number}`);
      } else {
        window.location.assign(treeHref(owner, repoName, res.data.branch, dir));
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
      You need write access to upload files.
    </p>;
  } @else {
    <>
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <PathBreadcrumb owner={owner} repo={repoName} refName={refName} path={dir} />
        <span className="text-[14px] text-muted-foreground">
          {(dir ? `— files land in ${dir}/` : "— files land at the repository root") as string}
        </span>
      </div>
      <div className="mb-4">
        <FileDropzone
          multiple={true}
          maxFiles={UPLOAD_MAX_FILES}
          maxSize={FILE_MAX_BYTES}
          disabled={pending}
          onFiles={(next) => addFiles(next)}
          onReject={(rejections) => {
            setError(
              rejections
                .map((r) => `${r.file.name}: ${r.errors.map((e) => e.code).join(", ")}`)
                .join("; "),
            );
          }}
          hint={`Up to ${UPLOAD_MAX_FILES} files, ${Math.round(FILE_MAX_BYTES / 1024 / 1024)} MB each.`}
        />
      </div>
      @if (files.length > 0) {
        <ul
          className="mb-4 divide-y divide-border rounded-md border border-border bg-card/40"
          data-testid="upload-staged"
        >
          @for (const f of files; key f.name) {
            <li className="flex min-h-10 items-center gap-3 px-3 py-1.5">
              <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-foreground">
                {joinRepoPath(dir, f.name) as string}
              </span>
              <span className="shrink-0 text-[12px] text-muted-foreground">
                {formatFileSize(f.size) as string}
              </span>
              <button
                type="button"
                className="shrink-0 text-[13px] text-muted-foreground underline hover:text-destructive"
                onClick={() => removeFile(f.name)}
                disabled={pending}
              >
                Remove
              </button>
            </li>;
          }
        </ul>;
      }
      <FileCommitForm
        policy={policyQ.data ?? null}
        policyPending={policyQ.isPending && canWrite}
        baseBranch={refName}
        pending={pending}
        error={error}
        submitLabel="Commit uploads"
        messagePlaceholder="Add files via upload"
        cancelHref={treeHref(owner, repoName, refName, dir) as string}
        onSubmit={submit}
      />
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoFileUploadPage() @{
  <AppPage>
    <RepoLayout>
      <RepoFileUploadPageContent />
    </RepoLayout>
  </AppPage>;
}
