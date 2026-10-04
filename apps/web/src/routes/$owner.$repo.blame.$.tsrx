import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import type { RepoBlameResponse, RepoPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { CommitAuthorLink } from "@/components/repo/commit-author-link";
import { PathBreadcrumb } from "@/components/repo/path-breadcrumb";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { commitHref, parseRefAndPath, shortRefName } from "@/lib/repo-browse";
import { formatRelativeUpdated } from "@/lib/relative-time";
import { fetchRepoBlame, fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type BlameLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | {
      kind: "ready";
      repo: RepoPublic;
      refName: string;
      path: string;
      blame: RepoBlameResponse;
    };

type Phase = { kind: "loading" } | BlameLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadBlamePage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<BlameLoaderData> {
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
  const knownRefs = refsRes.ok ? refsRes.data.refs.map((r) => shortRefName(r.name)) : [];
  const parsed = parseRefAndPath(splat, knownRefs);
  const refName = parsed.ref || repo.default_branch || "main";
  if (!parsed.path) {
    return { kind: "not_found" };
  }
  const blameRes = await fetchRepoBlame({
    data: { owner, name: repoName, ref: refName, path: parsed.path },
  });
  if (!blameRes.ok) {
    if (blameRes.error.code === "repo.not_found" || blameRes.error.code === "repo.path_not_found") {
      return { kind: "not_found" };
    }
    return {
      kind: "error",
      message: blameRes.error.message || "Could not load blame.",
    };
  }
  return {
    kind: "ready",
    repo,
    refName: blameRes.data.ref || refName,
    path: blameRes.data.path || parsed.path,
    blame: blameRes.data,
  };
}

export const Route = createFileRoute("/$owner/$repo/blame/$")({
  component: RepoBlamePage,
  head: ({ params }) => {
    const { path } = parseRefAndPath((params as { _splat?: string })._splat);
    const base = path.split("/").pop() || path || "file";
    const trunc = base.length > 40 ? `…${base.slice(-36)}` : base;
    return {
      meta: [{ title: `Blame · ${trunc} · Oxidean` }],
    };
  },
  loader: async ({ params }): Promise<BlameLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    const splat = (params as { _splat?: string })._splat;
    try {
      return await loadBlamePage(owner, repoName, splat);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

function shortSha(sha: string): string {
  return sha.slice(0, 7);
}

function relativeCommitTime(iso: string): string {
  return formatRelativeUpdated(iso).replace(/^Updated\s+/i, "");
}

export function RepoBlamePage() @{
  const params = useParams({ from: "/$owner/$repo/blame/$" }) as {
    owner: string;
    repo: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const splat = params._splat;
  const loaderData = useLoaderData({ from: "/$owner/$repo/blame/$" }) as
    | BlameLoaderData
    | undefined;

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
        const next = await loadBlamePage(owner, repoName, splat);
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
    <p className="text-[16px] text-muted-foreground">Loading blame…</p>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else {
    <>
      <PathBreadcrumb owner={owner} repo={repoName} refName={phase.refName} path={phase.path} />
      @if (phase.blame.truncated) {
        <p className="mb-2 text-[14px] text-muted-foreground">Blame truncated for size.</p>;
      }
      <div className="overflow-x-auto border border-border">
        <table className="w-full border-collapse text-left text-[14px] leading-[1.45]">
          <tbody>
            @for (const line of phase.blame.lines; key line.line_number) {
              <tr className="border-b border-border/60 align-top">
                <td className="whitespace-nowrap bg-muted/30 px-2 py-0.5 font-mono text-[12px] text-muted-foreground">
                  <AppLink
                    href={commitHref(owner, repoName, line.sha) as string}
                    className="text-primary no-underline hover:underline"
                    title={line.sha as string}
                  >
                    {shortSha(line.sha) as string}
                  </AppLink>
                  <span className="ml-2 inline-flex items-center font-[family-name:var(--font-sans)] text-muted-foreground">
                    <CommitAuthorLink
                      name={line.author_name}
                      username={line.author_username}
                      avatarUrl={line.author_avatar_url}
                    />
                  </span>
                  <time
                    className="ml-2 text-muted-foreground"
                    dateTime={line.authored_at as string}
                  >
                    {relativeCommitTime(line.authored_at) as string}
                  </time>
                </td>
                <td className="select-none px-2 py-0.5 font-mono text-[12px] text-muted-foreground">
                  {String(line.line_number) as string}
                </td>
                <td className="px-2 py-0.5 font-mono text-[14px] text-foreground">
                  <pre className="m-0 whitespace-pre-wrap font-mono">
                    {line.content as string}
                  </pre>
                </td>
              </tr>;
            }
          </tbody>
        </table>
      </div>
    </>;
  }
}
