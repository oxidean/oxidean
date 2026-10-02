import { createFileRoute, useLoaderData, useParams, useRouter } from "@octanejs/tanstack-router";
import type { RepoPublic, RepoRefEntry } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { FileArchive, Tag } from "@octanejs/lucide";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { List, ListRow } from "@/components/ui/list";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";
import {
  archiveTarGzUrl,
  archiveZipUrl,
  isTagRef,
  shortOid,
  shortRefName,
  treeHref,
} from "@/lib/repo-browse";
import { fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";
import { formatRelative } from "@/lib/relative-time";
import { AppLink } from "@/components/ui/app-link";

export type TagsLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "ready"; repo: RepoPublic; tags: RepoRefEntry[] };

type Phase = { kind: "loading" } | TagsLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function sortTags(refs: RepoRefEntry[]): RepoRefEntry[] {
  return refs
    .filter((r) => isTagRef(r.name))
    .sort((a, b) =>
      shortRefName(a.name).localeCompare(shortRefName(b.name), undefined, {
        sensitivity: "base",
      }),
    );
}

async function loadTagsPage(owner: string, repoName: string): Promise<TagsLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const refsRes = await fetchRepoRefs({ data: { owner, name: repoName } });
  if (!refsRes.ok) {
    if (refsRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: refsRes.error.message || "Could not load tags.",
    };
  }
  return {
    kind: "ready",
    repo: got.data,
    tags: sortTags(refsRes.data.refs),
  };
}

export const Route = createFileRoute("/$owner/$repo/tags")({
  component: RepoTagsPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Tags · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<TagsLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    try {
      return await loadTagsPage(owner, repoName);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Tags list + archive download only — no create-tag CTA (UI-SPEC lock). */
export function RepoTagsPage() @{
  const params = useParams({ from: "/$owner/$repo/tags" }) as {
    owner: string;
    repo: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const router = useRouter();
  const loaderData = useLoaderData({ from: "/$owner/$repo/tags" }) as TagsLoaderData | undefined;

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
        const next = await loadTagsPage(owner, repoName);
        if (!cancelled) setPhase(next);
      } catch {
        if (!cancelled) setPhase({ kind: "error", message: NETWORK_ERROR });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [owner, repoName, loaderData]);

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <>
      <PageHeader icon={<Tag aria-hidden size={22} />} title="Tags" />
      <List className="mt-6">
        @for (const i of [0, 1, 2]; key i) {
          <ListRow>
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-4 w-40" />
              <Skeleton className="h-3 w-20" />
            </div>
          </ListRow>;
        }
      </List>
    </>;
  } @else if (phase.kind === "error") {
    <div className="flex flex-col gap-4">
      <p role="alert" className="text-[16px] text-destructive">
        {phase.message as string}
      </p>
      <Button type="button" variant="outline" onClick={() => void router.invalidate()}>
        Try again
      </Button>
    </div>;
  } @else {
    <>
      <PageHeader icon={<Tag aria-hidden size={22} />} title="Tags" />
      @if (phase.tags.length === 0) {
        <EmptyState
          className="mt-6"
          icon={<Tag aria-hidden size={22} />}
          title="No tags yet"
          description="Tags mark release points in history. Push a tag to see it listed here."
        />;
      } @else {
        <List className="mt-6">
          @for (const t of phase.tags; key t.name) {
            <TagRow owner={owner} repo={repoName} tag={t} />;
          }
        </List>;
      }
    </>;
  }
}

function TagRow({
  owner,
  repo,
  tag,
}: {
  owner: string;
  repo: string;
  tag: RepoRefEntry;
}) @{
  const name = shortRefName(tag.name);
  const tip = shortOid(tag.oid);
  const href = treeHref(owner, repo, name, "");
  const zipHref = archiveZipUrl(owner, repo, name);
  const tarHref = archiveTarGzUrl(owner, repo, name);

  <ListRow className="flex-wrap gap-x-4 gap-y-2">
    <Tag aria-hidden size={16} className="shrink-0 text-muted-foreground" />
    <div className="min-w-0 flex-1">
      <AppLink
        href={href as string}
        className="block truncate text-[15px] font-semibold text-primary no-underline hover:underline"
        title={name as string}
      >
        {name as string}
      </AppLink>
      <p className="mt-0.5 font-mono text-[12px] text-muted-foreground" title={tag.oid as string}>
        {tip as string}
      </p>
      @if (tag.tip_committed_at) {
        <p className="mt-0.5 text-[12px] text-muted-foreground">
          {
            (formatRelative(tag.tip_committed_at) +
              (tag.tip_author_name ? ` by ${tag.tip_author_name}` : "")) as string
          }
        </p>;
      }
    </div>
    <div className="flex flex-wrap gap-2">
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => {
          window.location.assign(zipHref);
        }}
      >
        <FileArchive aria-hidden size={14} />
        ZIP
      </Button>
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => {
          window.location.assign(tarHref);
        }}
      >
        <FileArchive aria-hidden size={14} />
        tar.gz
      </Button>
    </div>
  </ListRow>;
}
