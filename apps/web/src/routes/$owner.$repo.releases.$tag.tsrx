import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useQuery } from "@octanejs/tanstack-query";
import type { ReleasePublic, RepoPublic } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { Badge } from "@/components/ui/badge";
import { FileDropzone } from "@/components/ui/file-dropzone";
import { Label } from "@/components/ui/label";
import { MarkdownHtml } from "@/components/repo/markdown-body";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { apiClient } from "@/lib/api-client";
import { renderGfm } from "@/lib/markdown";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";
import { fetchRepoGet } from "@/lib/ssr-repo";

export type ReleaseDetailLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "ready"; repo: RepoPublic; release: ReleasePublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadReleaseDetail(
  owner: string,
  repoName: string,
  tag: string,
): Promise<ReleaseDetailLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const rel = await apiClient.release.get({
    owner,
    name: repoName,
    tag_name: tag,
  });
  if (!rel.ok) {
    if (rel.error.code === "repo.not_found" || rel.error.code === "release.not_found") {
      return { kind: "not_found" };
    }
    return {
      kind: "error",
      message: rel.error.message || "Could not load release.",
    };
  }
  return { kind: "ready", repo: got.data, release: rel.data };
}

export const Route = createFileRoute("/$owner/$repo/releases/$tag")({
  component: RepoReleaseDetailPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Release ${(params as { tag: string }).tag} · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<ReleaseDetailLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    const tag = decodeURIComponent(String((params as { tag?: string }).tag ?? ""));
    try {
      return await loadReleaseDetail(owner, repoName, tag);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Release detail — notes via renderGfm; assets land in 15-02. */
export function RepoReleaseDetailPage() @{
  const params = useParams({ strict: false }) as {
    owner?: string;
    repo?: string;
    tag?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const tag = decodeURIComponent(params.tag ?? "");
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const routeLoader = useLoaderData({
    from: "/$owner/$repo/releases/$tag",
  }) as ReleaseDetailLoaderData | undefined;

  const [bodyHtml, setBodyHtml] = useState("");

  const repoQ = useQuery({
    queryKey: ["repo", "get", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.get({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName,
    initialData:
      routeLoader?.kind === "ready"
        ? routeLoader.repo
        : layout?.status === "ok"
          ? (layout.repo ?? undefined)
          : undefined,
  });

  const releaseQ = useQuery({
    queryKey: ["release", "get", owner, repoName, tag] as const,
    queryFn: async () => {
      const res = await apiClient.release.get({
        owner,
        name: repoName,
        tag_name: tag,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName && !!tag,
    initialData: routeLoader?.kind === "ready" ? routeLoader.release : undefined,
  });

  const repo = repoQ.data ?? null;
  const release = releaseQ.data ?? null;

  useEffect(() => {
    let cancelled = false;
    async function run() {
      if (!release) {
        setBodyHtml("");
        return;
      }
      const html = await renderGfm(release.body || "_No release notes._", {
        owner,
        repo: repoName,
      });
      if (!cancelled) setBodyHtml(html);
    }
    void run();
    return () => {
      cancelled = true;
    };
  });

  const notFound =
    layout?.status === "not_found" ||
    routeLoader?.kind === "not_found" ||
    (releaseQ.isError &&
      String(releaseQ.error?.message ?? "")
        .toLowerCase()
        .includes("not found"));

  @if (notFound || (!repoQ.isPending && !repo)) {
    <RepoNotFound />;
  } @else if (!repo || (!release && releaseQ.isPending)) {
    <p className="text-[16px] text-muted-foreground">Loading release…</p>;
  } @else if (!release) {
    <RepoNotFound />;
  } @else {
    <>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <h2 className="text-[22px] font-semibold text-foreground">
          {(release.title.trim() || release.tag_name) as string}
        </h2>
        @if (release.draft) {
          <Badge variant="outline">Draft</Badge>;
        }
        @if (release.prerelease) {
          <Badge variant="outline">Pre-release</Badge>;
        }
      </div>
      <p className="mb-6 text-[14px] text-muted-foreground">
        <span className="font-mono">{release.tag_name as string}</span>
        @if (release.author_username) {
          <>
            {" · "}
            {`by ${release.author_username}` as string}
          </>;
        }
      </p>
      <MarkdownHtml
        html={bodyHtml as string}
        className="text-foreground"
        data-testid="release-body"
      />
      <section className="mt-8">
        <h3 className="mb-3 text-[16px] font-semibold text-foreground">Assets</h3>
        @if (release.assets && release.assets.length > 0) {
          <ul className="mb-4 flex flex-col gap-2">
            @for (const a of release.assets; key a.id) {
              <li>
                <a
                  href={`/api/releases/assets/${a.id}` as string}
                  className="text-[14px] text-primary underline"
                >
                  {(a.filename || a.id) as string}
                </a>
                <span className="ml-2 text-[12px] text-muted-foreground">
                  {`${a.byte_size} bytes` as string}
                </span>
              </li>;
            }
          </ul>;
        } @else {
          <p className="mb-4 text-[14px] text-muted-foreground">No assets attached.</p>;
        }
        @if (repo.can_write && !repo.archived) {
          <form
            method="post"
            action="#"
            className="flex flex-col gap-2"
            onSubmit={(e) => {
              e.preventDefault();
            }}
          >
            <Label htmlFor="release-asset-file">Upload asset</Label>
            <FileDropzone
              id="release-asset-file"
              hint="Drop a release asset here, or click to browse"
              onFiles={(files) => {
                const file = files[0];
                if (!file || !release) return;
                void (async () => {
                  const fd = new FormData();
                  fd.append("asset", file, file.name);
                  const res = await fetch(
                    `/api/repos/${owner}/${repoName}/releases/${release.id}/assets`,
                    { method: "POST", body: fd, credentials: "include" },
                  );
                  if (res.ok) {
                    await releaseQ.refetch();
                  }
                })();
              }}
            />
          </form>;
        }
      </section>
      <p className="mt-8">
        <a
          href={`/${owner}/${repoName}/releases` as string}
          className="text-[14px] text-primary underline"
        >
          All releases
        </a>
      </p>
    </>;
  }
}
