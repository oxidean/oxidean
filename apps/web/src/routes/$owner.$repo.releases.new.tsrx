import { useForm } from "@octanejs/tanstack-form";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { navigate as astroNavigate } from "astro:transitions/client";
import { useRepoLayoutData } from "@/lib/repo-store.tsrx";
import { matchPath, routeHref } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoPublic, RepoRefEntry } from "@oxidean/api-client";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { isTagRef, shortRefName } from "@/lib/repo-browse";

import { fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type ReleaseNewLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "ready"; repo: RepoPublic; tags: string[] }
  | { kind: "forbidden"; repo: RepoPublic; tags: string[] };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function tagNames(refs: RepoRefEntry[]): string[] {
  return refs
    .filter((r) => isTagRef(r.name))
    .map((r) => shortRefName(r.name))
    .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
}

async function loadNewReleasePage(owner: string, repoName: string): Promise<ReleaseNewLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const refsRes = await fetchRepoRefs({ owner, name: repoName });
  const tags = refsRes.ok ? tagNames(refsRes.data.refs) : [];
  if (!got.data.can_write) {
    return { kind: "forbidden", repo: got.data, tags };
  }
  return { kind: "ready", repo: got.data, tags };
}

function repoReleasesNewQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/releases/new",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<ReleaseNewLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      try {
        return await loadNewReleasePage(owner, repoName);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Create release from an existing tag (D-REL-01 / D-REL-13) — notes only. */
function RepoReleaseNewPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/releases/new", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const navigate = astroNavigate;
  const layout = useRepoLayoutData();
  const routeLoader = useQuery(repoReleasesNewQueryOptions(params, searchStr)).data;

  const initialTags =
    routeLoader?.kind === "ready" || routeLoader?.kind === "forbidden" ? routeLoader.tags : [];

  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  const form = useForm({
    defaultValues: {
      tagName: initialTags[0] ?? "",
      title: "",
      body: "",
      draft: false,
      prerelease: false,
    },
    onSubmit: async ({ value }) => {
      setError("");
      if (!value.tagName.trim()) {
        setError("Choose an existing tag.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.release.create({
          owner,
          name: repoName,
          tag_name: value.tagName.trim(),
          title: value.title.trim() || value.tagName.trim(),
          body: value.body || undefined,
          draft: value.draft,
          prerelease: value.prerelease,
        });
        if (!res.ok) {
          setError(res.error.message || "Could not create release.");
          setPending(false);
          return;
        }
        await navigate(
          routeHref("/$owner/$repo/releases/$tag", {
            owner,
            repo: repoName,
            tag: res.data.tag_name,
          }),
        );
      } catch {
        setError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  const repoQ = useQuery({
    queryKey: ["repo", "get", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.get({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName,
    initialData:
      routeLoader?.kind === "ready" || routeLoader?.kind === "forbidden"
        ? routeLoader.repo
        : layout?.status === "ok"
          ? (layout.repo ?? undefined)
          : undefined,
  });

  const tagsQ = useQuery({
    queryKey: ["repo", "refs", "tags", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.refs({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return tagNames(res.data.refs);
    },
    enabled: !!owner && !!repoName,
    initialData:
      routeLoader?.kind === "ready" || routeLoader?.kind === "forbidden"
        ? routeLoader.tags
        : undefined,
  });

  const repo = repoQ.data ?? null;
  const tags = tagsQ.data ?? [];

  @if (layout?.status === "not_found" || (!repoQ.isPending && !repo)) {
    <RepoNotFound />;
  } @else if (!repo) {
    <p className="text-[16px] text-muted-foreground">Loading…</p>;
  } @else if (repo.archived) {
    <p role="alert" className="text-[16px] text-muted-foreground">
      This repository is archived and read-only. Releases can't be created.
    </p>;
  } @else if (!repo.can_write) {
    <p role="alert" className="text-[16px] text-destructive">
      You need write access to create releases.
    </p>;
  } @else {
    <>
      <h2 className="mb-4 text-[18px] font-semibold text-foreground">New release</h2>
      <form
        method="post"
        action="#"
        className="flex max-w-2xl flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          void form.handleSubmit();
        }}
      >
        @if (error) {
          <AuthErrorBanner message={error} />;
        }
        <form.Field name="tagName">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="release-tag">Tag</Label>
              @if (tags.length === 0) {
                <p className="text-[14px] text-muted-foreground">
                  No tags yet. Push a tag before creating a release.
                </p>;
              } @else {
                <select
                  id="release-tag"
                  className="h-10 rounded-md border border-border bg-background px-3 text-[14px] text-foreground"
                  value={field.state.value}
                  onChange={(e) => field.handleChange((e.target as HTMLSelectElement).value)}
                >
                  @for (const t of tags; key t) {
                    <option value={t as string}>{t as string}</option>;
                  }
                </select>;
              }
            </div>
          )}
        </form.Field>
        <form.Field name="title">
          {(field) => (
            <div className="flex flex-col gap-2">
              <Label htmlFor="release-title">Release title</Label>
              <Input
                id="release-title"
                name="title"
                value={field.state.value}
                onBlur={field.handleBlur}
                onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                placeholder="Optional — defaults to the tag name"
              />
            </div>
          )}
        </form.Field>
        <form.Field name="body">
          {(field) => (
            <div className="flex flex-col gap-2">
              <MarkdownWritePreview
                id="release-body"
                value={field.state.value}
                onValueChange={field.handleChange}
                rows={12}
                disabled={pending}
                placeholder="Describe this release (markdown supported)"
                owner={owner}
                repo={repoName}
              />
            </div>
          )}
        </form.Field>
        <form.Subscribe selector={(s) => s.values}>
          {(values) => (
            <>
              <label className="flex items-center gap-2 text-[14px] text-foreground">
                <input
                  type="checkbox"
                  checked={values.draft}
                  onChange={(e) =>
                    form.setFieldValue("draft", (e.target as HTMLInputElement).checked)
                  }
                />
                Save as draft
              </label>
              <label className="flex items-center gap-2 text-[14px] text-foreground">
                <input
                  type="checkbox"
                  checked={values.prerelease}
                  onChange={(e) =>
                    form.setFieldValue("prerelease", (e.target as HTMLInputElement).checked)
                  }
                />
                Set as a pre-release
              </label>
            </>
          )}
        </form.Subscribe>
        <div className="flex gap-3">
          <Button type="submit" disabled={pending || tags.length === 0}>
            {(pending ? "Publishing…" : "Publish release") as string}
          </Button>
          <AppLink
            href={`/${owner}/${repoName}/releases` as string}
            className="text-[14px] text-muted-foreground underline"
          >
            Cancel
          </AppLink>
        </div>
      </form>
    </>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function RepoReleaseNewPage() @{
  <AppPage>
    <RepoLayout>
      <RepoReleaseNewPageContent />
    </RepoLayout>
  </AppPage>;
}
