import type { RepoFileTemplate, RepoTemplatesListResponse } from "@oxidean/api-client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { navigate as astroNavigate } from "astro:transitions/client";

import { matchPath, routeHref, searchRecord } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useEffect, useState } from "octane";
import { MemberLookup } from "@/components/org/member-lookup";
import { FileTemplateChooser } from "@/components/repo/file-template-chooser";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { apiClient } from "@/lib/api-client";
import { fetchRepoFileTemplates, fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";
const EMPTY_TEMPLATES: RepoTemplatesListResponse = { issues: [], pulls: [] };

type PullsNewLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  // Pulls unit disabled for this repo (COL-13).
  | {
      kind: "disabled";
      repo: { can_admin?: boolean; archived?: boolean };
    }
  | {
      kind: "ready";
      repo: {
        default_branch: string;
        can_write?: boolean;
        can_admin?: boolean;
        archived?: boolean;
      };
      refs: { name: string }[];
      templates: RepoTemplatesListResponse;
    };

/** `?template=` selects by repo-relative filename or basename (GitHub deep-link). */
function matchTemplate(
  templates: RepoFileTemplate[],
  param: string | undefined,
): RepoFileTemplate | null {
  const want = param?.trim();
  if (!want) return null;
  return (
    templates.find((t) => t.filename === want) ??
    templates.find((t) => t.filename.split("/").pop() === want) ??
    null
  );
}

function validateSearch(search: Record<string, unknown>) {
  return {
    base: typeof search.base === "string" ? search.base : undefined,
    head: typeof search.head === "string" ? search.head : undefined,
    head_owner: typeof search.head_owner === "string" ? search.head_owner : undefined,
    template: typeof search.template === "string" ? search.template : undefined,
  };
}

function repoPullsNewQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/pulls/new",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<PullsNewLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const got = await fetchRepoGet({ owner, name: repoName });
      if (!got.ok) {
        if (got.error.code === "repo.not_found") return { kind: "not_found" as const };
        return { kind: "error" as const, message: got.error.message || NETWORK_ERROR };
      }
      // Unit toggle (COL-13): Pulls off → disabled view, skip ref enumeration.
      if (got.data.pulls_enabled === false) {
        return { kind: "disabled" as const, repo: got.data };
      }
      const [refs, tpl] = await Promise.all([
        fetchRepoRefs({ owner, name: repoName }),
        // COL-02: template listing failure must not block the create form.
        fetchRepoFileTemplates({ owner, name: repoName }).catch(() => null),
      ]);
      return {
        kind: "ready" as const,
        repo: got.data,
        refs: refs.ok ? refs.data.refs : [],
        templates: tpl?.ok ? tpl.data : EMPTY_TEMPLATES,
      };
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/**
 * Create pull request (Phase 12): pick base + head refs, title, body (Markdown
 * Write/Preview), plus the COL-02 file-template chooser.
 */
function NewPullPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/pulls/new", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const search = validateSearch(searchRecord(searchStr)) as {
    base?: string;
    head?: string;
    head_owner?: string;
    template?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const navigate = astroNavigate;
  const loader = useQuery(repoPullsNewQueryOptions(params, searchStr)).data;

  const pullTemplates = loader?.kind === "ready" ? (loader.templates?.pulls ?? []) : [];
  const requestedTemplate = matchTemplate(pullTemplates, search.template);
  const initialTemplate =
    requestedTemplate ?? (pullTemplates.length === 1 ? pullTemplates[0] : null);

  const defaultBranch = loader?.kind === "ready" ? loader.repo.default_branch || "main" : "main";
  const [title, setTitle] = useState(initialTemplate?.title ?? "");
  const [body, setBody] = useState(initialTemplate?.body ?? "");
  const [baseRef, setBaseRef] = useState(search.base || defaultBranch);
  const [headRef, setHeadRef] = useState(search.head || defaultBranch);
  const [headOwner, setHeadOwner] = useState(search.head_owner || owner);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Chooser resolves when a template (or blank) is picked — or `?template=` /
  // a single template already picked one. Seeding `false` is safe: the
  // chooser only renders when >1 template exists, and the effect below
  // auto-picks `?template=`/single-template cases once the query resolves.
  const [templateChosen, setTemplateChosen] = useState(initialTemplate !== null);
  const [appliedTemplate, setAppliedTemplate] = useState<RepoFileTemplate | null>(initialTemplate);

  const branches =
    loader?.kind === "ready"
      ? loader.refs
          .filter((r) => r.name.startsWith("refs/heads/"))
          .map((r) => r.name.replace(/^refs\/heads\//, ""))
      : [];

  function pickPullTemplate(t: RepoFileTemplate | null) {
    if (t) {
      setTitle(t.title ?? "");
      setBody(t.body);
      setAppliedTemplate(t);
    } else {
      setAppliedTemplate(null);
    }
    setTemplateChosen(true);
  }

  // Covers client-side navigations where loader data arrives after mount.
  useEffect(() => {
    if (templateChosen) return;
    const t = requestedTemplate ?? (pullTemplates.length === 1 ? pullTemplates[0] : null);
    if (t) pickPullTemplate(t);
  }, [pullTemplates, requestedTemplate, templateChosen]);

  async function onSubmit() {
    setBusy(true);
    setError(null);
    try {
      const headOwnerTrim = headOwner.trim() || owner;
      const res = await apiClient.pull.create({
        owner,
        name: repoName,
        title,
        body,
        base_ref: baseRef,
        head_ref: headRef,
        head_owner: headOwnerTrim === owner ? undefined : headOwnerTrim,
      });
      if (!res.ok) {
        setError(res.error.message || NETWORK_ERROR);
        setBusy(false);
        return;
      }
      await navigate(
        routeHref("/$owner/$repo/pull/$n", { owner, repo: repoName, n: String(res.data.number) }),
      );
    } catch {
      setError(NETWORK_ERROR);
      setBusy(false);
    }
  }

  @if (loader?.kind === "not_found") {
    <RepoNotFound />;
  } @else if (!loader) {
    <p className="text-[16px] text-muted-foreground">Loading…</p>;
  } @else if (loader.kind === "error") {
    <p className="text-destructive">{loader.message as string}</p>;
  } @else if (loader.repo.archived) {
    <p role="alert" className="text-[16px] text-muted-foreground">
      This repository is archived and read-only. Pull requests can't be opened.
    </p>;
  } @else if (loader.kind === "disabled") {
    <RepoUnitDisabled unit="pulls" canAdmin={loader.repo.can_admin ?? false} />;
  } @else {
    <div className="mx-auto max-w-2xl space-y-4">
      <h2 className="text-xl font-semibold">Open a pull request</h2>
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-col gap-1 text-sm">
          Base
          <select
            className="rounded-md border border-border bg-background px-2 py-1.5"
            value={baseRef as string}
            onChange={(e: Event) => setBaseRef((e.target as HTMLSelectElement).value)}
          >
            @for (const b of branches; key b) {
              <option value={b as string}>{b as string}</option>;
            }
          </select>
        </label>
        <MemberLookup
          id="pull-new-head-owner"
          name="head_owner"
          label="Head repo owner"
          compact
          placeholder={owner as string}
          value={headOwner as string}
          onValueChange={setHeadOwner}
        />
        <label className="flex flex-col gap-1 text-sm">
          Compare
          <select
            className="rounded-md border border-border bg-background px-2 py-1.5"
            value={headRef as string}
            onChange={(e: Event) => setHeadRef((e.target as HTMLSelectElement).value)}
          >
            @for (const b of branches; key b) {
              <option value={b as string}>{b as string}</option>;
            }
          </select>
        </label>
      </div>
      <p className="text-sm text-muted-foreground">
        To compare across forks, set Head repo owner to the fork's owner — the compare branch must
        exist there.
      </p>
      @if (pullTemplates.length > 1 && !templateChosen) {
        <FileTemplateChooser
          kindLabel="pull request"
          templates={pullTemplates}
          onPick={pickPullTemplate}
          testId="pull-template-chooser"
          blankTestId="pull-template-blank"
        />;
      } @else {
        <>
          @if (pullTemplates.length > 1 && templateChosen) {
            <p className="text-[13px] text-muted-foreground">
              {
                (appliedTemplate
                  ? `Template: ${appliedTemplate.name}`
                  : "Blank pull request") as string
              }
              {" · " as string}
              <button
                type="button"
                className="text-foreground underline underline-offset-4 hover:text-primary"
                onClick={() => setTemplateChosen(false)}
              >
                Change template
              </button>
            </p>;
          }
          <label className="flex flex-col gap-1 text-sm">
            Title
            <input
              className="rounded-md border border-border bg-background px-3 py-2"
              value={title as string}
              onInput={(e: Event) => setTitle((e.target as HTMLInputElement).value)}
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            Body
            <MarkdownWritePreview
              value={body}
              onValueChange={setBody}
              placeholder="Describe this pull request (markdown supported)"
              rows={8}
              disabled={busy}
              owner={owner}
              repo={repoName}
            />
          </label>
          @if (error) {
            <p className="text-sm text-destructive">{error as string}</p>;
          }
          <button
            type="button"
            className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground disabled:opacity-50"
            disabled={busy || !title.trim()}
            onClick={() => void onSubmit()}
          >
            Create pull request
          </button>
        </>;
      }
    </div>;
  }
}

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function NewPullPage() @{
  <AppPage>
    <RepoLayout>
      <NewPullPageContent />
    </RepoLayout>
  </AppPage>;
}
