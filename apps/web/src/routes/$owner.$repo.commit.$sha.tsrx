import type { RepoCommitResponse, RepoPublic } from "@oxidean/api-client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useEffect, useState } from "octane";
import { FileDiff } from "@octanejs/lucide";
import { CommitAuthorLink } from "@/components/repo/commit-author-link";
import { CommitVerifiedBadge } from "@/components/repo/commit-verified-badge";
import { DiffPatch } from "@/components/repo/diff-patch";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import type { HighlightTheme } from "@/lib/highlight";
import type { HighlightedDiffLine } from "@/lib/highlight-diff";
import { formatRelativeUpdated } from "@/lib/relative-time";
import { ssrHighlightDiffFiles } from "@/lib/ssr-diff-highlight";
import { fetchRepoCommit, fetchRepoGet } from "@/lib/ssr-repo";

export type CommitLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      commit: RepoCommitResponse;
      /** Pre-highlighted diff rows by path (SSR). */
      highlightedByPath: Record<string, HighlightedDiffLine[]>;
      highlightTheme: HighlightTheme | null;
    };

type Phase = { kind: "loading" } | CommitLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadCommitPage(
  owner: string,
  repoName: string,
  sha: string,
): Promise<CommitLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const detail = await fetchRepoCommit({ owner, name: repoName, sha });
  if (!detail.ok) {
    if (detail.error.code === "repo.not_found" || detail.error.code === "repo.path_not_found") {
      return { kind: "not_found" };
    }
    return {
      kind: "error",
      message: detail.error.message || "Could not load commit.",
    };
  }
  const hl = await ssrHighlightDiffFiles(detail.data.files);
  return {
    kind: "ready",
    repo: got.data,
    commit: detail.data,
    highlightedByPath: hl.byPath,
    highlightTheme: hl.theme,
  };
}

function repoCommitShaQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/commit/$sha",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<CommitLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const sha = String((params as { sha?: string }).sha ?? "");
      try {
        return await loadCommitPage(owner, repoName, sha);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function relativeCommitTime(iso: string): string {
  return formatRelativeUpdated(iso).replace(/^Updated\s+/i, "");
}

function RepoCommitPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/commit/$sha", pathname) ?? {}) as {
    owner: string;
    repo: string;
    sha: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const sha = params.sha ?? "";
  const loaderData = useQuery(repoCommitShaQueryOptions(params, searchStr)).data;

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
        const next = await loadCommitPage(owner, repoName, sha);
        if (!cancelled) setPhase(next);
      } catch {
        if (!cancelled) setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, sha, loaderData]);

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2 border-b border-border pb-4">
        <Skeleton className="h-6 w-2/3" />
        <Skeleton className="h-4 w-48" />
      </div>
      <Skeleton className="h-40 w-full" />
    </div>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else {
    <>
      <header className="mb-6 border-b border-border pb-4">
        <h2 className="text-[20px] font-semibold text-foreground">
          {phase.commit.subject as string}
        </h2>
        @if (phase.commit.body.trim()) {
          <pre className="mt-2 whitespace-pre-wrap font-[family-name:var(--font-sans)] text-[16px] text-muted-foreground">
            {phase.commit.body as string}
          </pre>;
        }
        <p className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-[14px] text-muted-foreground">
          <CommitAuthorLink
            name={phase.commit.author_name}
            username={phase.commit.author_username}
            avatarUrl={phase.commit.author_avatar_url}
          />
          <CommitVerifiedBadge
            status={phase.commit.signature_status}
            kind={phase.commit.signature_kind}
          />
          <span aria-hidden="true">·</span>
          <time dateTime={phase.commit.authored_at as string}>
            {relativeCommitTime(phase.commit.authored_at) as string}
          </time>
          <span aria-hidden="true">·</span>
          <code className="font-mono text-[13px]">{phase.commit.short_sha as string}</code>
        </p>
        @if (phase.commit.truncated) {
          <p className="mt-2 text-[14px] text-muted-foreground">Diff truncated for size.</p>;
        }
      </header>
      @if (phase.commit.files.length === 0) {
        <EmptyState
          icon={<FileDiff aria-hidden size={22} />}
          title="No file changes"
          description="This commit doesn't touch any files (it may be an empty or merge commit)."
        />;
      } @else {
        <div className="flex flex-col gap-6">
          @for (const f of phase.commit.files; key f.path) {
            <DiffPatch
              path={f.path as string}
              status={f.status as string}
              patch={f.patch as string}
              initialRows={phase.highlightedByPath[f.path] ?? null}
              highlightTheme={phase.highlightTheme}
            />;
          }
        </div>;
      }
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoCommitPage() @{
  <AppPage>
    <RepoLayout>
      <RepoCommitPageContent />
    </RepoLayout>
  </AppPage>;
}
