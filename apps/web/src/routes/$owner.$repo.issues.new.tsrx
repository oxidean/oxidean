import { useForm } from "@octanejs/tanstack-form";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";
import { navigate as astroNavigate } from "astro:transitions/client";
import { useRepoLayoutData } from "@/lib/repo-store.tsrx";
import { matchPath, routeHref, searchRecord } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoFileTemplate, RepoPublic, RepoTemplatesListResponse } from "@oxidean/api-client";
import { useEffect, useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { FileTemplateChooser } from "@/components/repo/file-template-chooser";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";

import { fetchRepoFileTemplates, fetchRepoGet } from "@/lib/ssr-repo";
import { useChromeAccountState } from "@/lib/use-chrome-account";

const EMPTY_TEMPLATES: RepoTemplatesListResponse = { issues: [], pulls: [] };

export type IssuesNewLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  // Issues unit disabled for this repo (COL-13).
  | { kind: "disabled"; repo: RepoPublic }
  | { kind: "ready"; repo: RepoPublic; templates: RepoTemplatesListResponse };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadNewIssuePage(owner: string, repoName: string): Promise<IssuesNewLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  // Unit toggle (COL-13): Issues off → disabled view before template fetch.
  if (got.data.issues_enabled === false) {
    return { kind: "disabled", repo: got.data };
  }
  // COL-02: template listing failure must not block the create form.
  const tpl = await fetchRepoFileTemplates({ owner, name: repoName }).catch(() => null);
  return { kind: "ready", repo: got.data, templates: tpl?.ok ? tpl.data : EMPTY_TEMPLATES };
}

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
  return { template: typeof search.template === "string" ? search.template : undefined };
}

function repoIssuesNewQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/issues/new",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<IssuesNewLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      try {
        return await loadNewIssuePage(owner, repoName);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

/** Create issue form with Write | Preview (D-ISS-10 / D-ISS-19) + COL-02 template chooser. */
function IssuesNewPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/issues/new", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const search = validateSearch(searchRecord(searchStr)) as { template?: string };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const navigate = astroNavigate;
  const account = useChromeAccountState();
  const layout = useRepoLayoutData();
  const routeLoader = useQuery(repoIssuesNewQueryOptions(params, searchStr)).data;

  const issueTemplates = routeLoader?.kind === "ready" ? (routeLoader.templates?.issues ?? []) : [];
  const requestedTemplate = matchTemplate(issueTemplates, search.template);
  const initialTemplate =
    requestedTemplate ?? (issueTemplates.length === 1 ? issueTemplates[0] : null);

  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  // Chooser resolves when a template (or blank) is picked — or `?template=` /
  // a single template auto-picks via the effect below. Seeding must not read
  // `issueTemplates`: the list is empty while the page query is still pending.
  const [templateChosen, setTemplateChosen] = useState(initialTemplate !== null);
  const [appliedTemplate, setAppliedTemplate] = useState<RepoFileTemplate | null>(initialTemplate);

  const form = useForm({
    defaultValues: {
      title: initialTemplate?.title ?? "",
      body: initialTemplate?.body ?? "",
    },
    onSubmit: async ({ value }) => {
      setError("");
      if (!value.title.trim()) {
        setError("Title is required.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.issue.create({
          owner,
          name: repoName,
          title: value.title.trim(),
          body: value.body || undefined,
        });
        if (!res.ok) {
          setError(res.error.message || "Could not create issue.");
          setPending(false);
          return;
        }
        await navigate(
          routeHref("/$owner/$repo/issues/$n", {
            owner,
            repo: repoName,
            n: String(res.data.number),
          }),
        );
      } catch {
        setError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  function pickIssueTemplate(t: RepoFileTemplate | null) {
    if (t) {
      form.setFieldValue("title", t.title ?? "");
      form.setFieldValue("body", t.body);
      setAppliedTemplate(t);
    } else {
      setAppliedTemplate(null);
    }
    setTemplateChosen(true);
  }

  // Covers client-side navigations where loader data arrives after mount.
  useEffect(() => {
    if (templateChosen) return;
    const t = requestedTemplate ?? (issueTemplates.length === 1 ? issueTemplates[0] : null);
    if (t) pickIssueTemplate(t);
  }, [issueTemplates, requestedTemplate, templateChosen]);

  const repoQ = useQuery({
    queryKey: ["repo", "get", owner, repoName] as const,
    queryFn: async () => {
      const res = await apiClient.repo.get({ owner, name: repoName });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data;
    },
    enabled: !!owner && !!repoName,
    initialData:
      routeLoader?.kind === "ready" || routeLoader?.kind === "disabled"
        ? routeLoader.repo
        : layout?.status === "ok"
          ? (layout.repo ?? undefined)
          : undefined,
  });

  const repo = repoQ.data ?? null;
  const unitDisabled = routeLoader?.kind === "disabled" || repo?.issues_enabled === false;

  @if (layout?.status === "not_found" || (!repoQ.isPending && !repo)) {
    <RepoNotFound />;
  } @else if (!repo) {
    <p className="text-[16px] text-muted-foreground">Loading…</p>;
  } @else if (repo.archived) {
    <p role="alert" className="text-[16px] text-muted-foreground">
      This repository is archived and read-only. Issues can't be created.
    </p>;
  } @else if (unitDisabled) {
    <RepoUnitDisabled unit="issues" canAdmin={repo?.can_admin ?? false} />;
  } @else if (!account.user) {
    <p className="rounded-md border border-border bg-muted/30 px-4 py-3 text-[13px] text-muted-foreground">
      <AppLink href="/login" className="font-medium text-primary no-underline hover:underline">
        Sign in
      </AppLink>
      {" to create an issue." as string}
    </p>;
  } @else if (!account.user.email_verified) {
    <p className="rounded-md border border-border bg-muted/30 px-4 py-3 text-[13px] text-muted-foreground">
      {"Verify your email to create issues." as string}
    </p>;
  } @else {
    <>
      <h2 className="mb-4 text-[18px] font-semibold text-foreground">New issue</h2>
      @if (issueTemplates.length > 1 && !templateChosen) {
        <FileTemplateChooser
          kindLabel="issue"
          templates={issueTemplates}
          onPick={pickIssueTemplate}
          testId="issue-template-chooser"
          blankTestId="issue-template-blank"
        />;
      } @else {
        <>
          @if (issueTemplates.length > 1) {
            <p className="mb-4 max-w-2xl text-[13px] text-muted-foreground">
              {
                (appliedTemplate
                  ? `Template: ${appliedTemplate.name}`
                  : "Blank issue") as string
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
            <form.Field name="title">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="issue-title">Title</Label>
                  <Input
                    id="issue-title"
                    name="title"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                    placeholder="Issue title"
                    required
                  />
                </div>
              )}
            </form.Field>
            <form.Field name="body">
              {(field) => (
                <div className="flex flex-col gap-2">
                  <MarkdownWritePreview
                    id="issue-body"
                    value={field.state.value}
                    onValueChange={field.handleChange}
                    rows={12}
                    disabled={pending}
                    placeholder="Describe the issue (markdown supported)"
                    owner={owner}
                    repo={repoName}
                  />
                </div>
              )}
            </form.Field>
            <div className="flex gap-3">
              <Button type="submit" disabled={pending}>
                {(pending ? "Creating…" : "Submit new issue") as string}
              </Button>
              <AppLink
                href={`/${owner}/${repoName}/issues` as string}
                className="inline-flex h-11 items-center text-[14px] text-muted-foreground no-underline hover:text-foreground"
              >
                Cancel
              </AppLink>
            </div>
          </form>
        </>;
      }
    </>;
  }
}

export default IssuesNewPage;

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function IssuesNewPage() @{
  <AppPage>
    <RepoLayout>
      <IssuesNewPageContent />
    </RepoLayout>
  </AppPage>;
}
