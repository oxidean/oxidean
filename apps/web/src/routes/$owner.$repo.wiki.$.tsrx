import type { RepoPublic, WikiPagePublic, WikiRevisionPublic } from "@oxidean/api-client";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import { useState } from "octane";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { BookOpen, FilePlus2, History, Pencil, Trash2 } from "@octanejs/lucide";
import { MarkdownBody, MarkdownHtml } from "@/components/repo/markdown-body";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { AppLink } from "@/components/ui/app-link";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { navigate } from "@/lib/nav";
import { formatRelative } from "@/lib/relative-time";
import { renderGfm } from "@/lib/markdown";
import { fetchRepoGet } from "@/lib/ssr-repo";
import { toastSuccess } from "@/lib/toast";

/** View parsed out of the `/wiki/*` splat — GitHub-style `_edit`/`_history`/`_new`. */
type WikiView = "index" | "page" | "new" | "edit" | "history";

type WikiLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "disabled"; repo: RepoPublic }
  | {
      kind: "ready";
      repo: RepoPublic;
      view: WikiView;
      /** Page name for page/edit/history views ("" for index/new). */
      pageName: string;
      pages: WikiPagePublic[];
      /** Loaded page for page/edit views and the index's Home fallback. */
      page: WikiPagePublic | null;
      /** Requested page does not exist — GitHub shows a create prompt. */
      pageMissing: boolean;
      revisions: WikiRevisionPublic[];
      /** SSR-rendered main page content. */
      markdownHtml: string | null;
      headerHtml: string | null;
      sidebarHtml: string | null;
      footerHtml: string | null;
    };

type Phase = { kind: "loading" } | WikiLoaderData;

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

const RESERVED_CHROME = ["_Header", "_Sidebar", "_Footer"] as const;

function parseWikiSplat(splat: string | undefined): { view: WikiView; pageName: string } | null {
  const segs = (splat ?? "")
    .split("/")
    .filter(Boolean)
    .map((s) => decodeURIComponent(s));
  if (segs.length === 0) return { view: "index", pageName: "" };
  if (segs.length === 1) {
    if (segs[0] === "_new") return { view: "new", pageName: "" };
    return { view: "page", pageName: segs[0] ?? "" };
  }
  if (segs.length === 2 && segs[1] === "_edit") {
    return { view: "edit", pageName: segs[0] ?? "" };
  }
  if (segs.length === 2 && segs[1] === "_history") {
    return { view: "history", pageName: segs[0] ?? "" };
  }
  return null;
}

async function loadWikiPage(
  owner: string,
  repoName: string,
  splat: string | undefined,
): Promise<WikiLoaderData> {
  const got = await fetchRepoGet({ owner, name: repoName });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  const repo = got.data;
  if (repo.wiki_enabled === false) return { kind: "disabled", repo };

  const parsed = parseWikiSplat(splat);
  if (!parsed) return { kind: "not_found" };
  const { view, pageName } = parsed;

  const listRes = await apiClient.wiki.pages.list({ owner, name: repoName });
  if (!listRes.ok) {
    if (listRes.error.code === "repo.wiki.disabled") return { kind: "disabled", repo };
    if (listRes.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: listRes.error.message || "Could not load wiki pages.",
    };
  }
  const pages = listRes.data.pages;

  // Chrome pages (`_Header`/`_Sidebar`/`_Footer`) render around the content —
  // GitHub parity. Each is optional; absent pages just leave the slot empty.
  const chromeHtml: Record<string, string | null> = {
    _Header: null,
    _Sidebar: null,
    _Footer: null,
  };
  for (const chrome of RESERVED_CHROME) {
    if (!pages.some((p) => p.page_name === chrome)) continue;
    const res = await apiClient.wiki.pages.get({ owner, name: repoName, page: chrome });
    if (res.ok && res.data.page.content) {
      try {
        chromeHtml[chrome] = await renderGfm(res.data.page.content, { owner, repo: repoName });
      } catch {
        chromeHtml[chrome] = null;
      }
    }
  }

  const wantPage = view === "page" || view === "edit" || view === "history";
  // The index renders the Home page inline when it exists (GitHub `/wiki`).
  const target = wantPage ? pageName : view === "index" ? "Home" : "";
  let page: WikiPagePublic | null = null;
  let pageMissing = false;
  if (target) {
    const res = await apiClient.wiki.pages.get({ owner, name: repoName, page: target });
    if (res.ok) {
      page = res.data.page;
    } else if (res.error.code === "wiki.not_found" || res.error.code === "git.not_found") {
      pageMissing = true;
    } else if (res.error.code === "repo.wiki.disabled") {
      return { kind: "disabled", repo };
    } else {
      return { kind: "error", message: res.error.message || "Could not load page." };
    }
  }

  let revisions: WikiRevisionPublic[] = [];
  if (view === "history" && page) {
    const res = await apiClient.wiki.revisions.list({
      owner,
      name: repoName,
      page: pageName,
      limit: 50,
    });
    if (res.ok) revisions = res.data.revisions;
  }

  let markdownHtml: string | null = null;
  if (page?.content && (view === "page" || view === "index")) {
    try {
      markdownHtml = await renderGfm(page.content, { owner, repo: repoName });
    } catch {
      markdownHtml = null;
    }
  }

  return {
    kind: "ready",
    repo,
    view,
    pageName,
    pages,
    page,
    pageMissing,
    revisions,
    markdownHtml,
    headerHtml: chromeHtml._Header ?? null,
    sidebarHtml: chromeHtml._Sidebar ?? null,
    footerHtml: chromeHtml._Footer ?? null,
  };
}

function repoWikiQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/wiki/$",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<WikiLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const splat = (params as { _splat?: string })._splat;
      try {
        return await loadWikiPage(owner, repoName, splat);
      } catch {
        return { kind: "error", message: NETWORK_ERROR };
      }
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function pageHref(owner: string, repoName: string, pageName: string): string {
  return `/${owner}/${repoName}/wiki/${encodeURIComponent(pageName)}`;
}

/** Right rail: optional `_Sidebar` content + the full Pages list (GitHub layout). */
function WikiSidebar(props: {
  owner: string;
  repoName: string;
  pages: WikiPagePublic[];
  sidebarHtml: string | null;
  canWrite: boolean;
  currentPage: string;
}) @{
  const base = `/${props.owner}/${props.repoName}/wiki`;
  <aside className="flex w-full shrink-0 flex-col gap-4 lg:w-64" data-testid="wiki-sidebar">
    @if (props.canWrite) {
      <AppLink
        href={`${base}/_new`}
        className="inline-flex h-8 items-center justify-center gap-1.5 rounded-md bg-primary px-3 text-[13px] font-medium text-primary-foreground no-underline hover:bg-primary/90"
      >
        <FilePlus2 aria-hidden size={14} />
        New page
      </AppLink>;
    }
    @if (props.sidebarHtml) {
      <MarkdownHtml
        html={props.sidebarHtml ?? ""}
        className="rounded-md border border-border p-3"
        data-testid="wiki-custom-sidebar"
      />;
    }
    <div className="rounded-md border border-border">
      <h3 className="border-b border-border px-3 py-2 text-[13px] font-semibold text-foreground">
        Pages
      </h3>
      @if (props.pages.length === 0) {
        <p className="px-3 py-2 text-[13px] text-muted-foreground">No pages yet.</p>;
      } @else {
        <ul className="flex flex-col py-1">
          @for (const p of props.pages.filter((p) => !p.reserved); key p.page_name) {
            <li>
              <AppLink
                href={pageHref(props.owner, props.repoName, p.page_name)}
                className={
                  (p.page_name === props.currentPage
                    ? "block px-3 py-1.5 text-[13px] font-semibold text-foreground no-underline"
                    : "block px-3 py-1.5 text-[13px] text-primary no-underline hover:bg-muted") as string
                }
              >
                {p.page_name as string}
              </AppLink>
            </li>;
          }
        </ul>;
      }
    </div>
  </aside>;
}

/** Create / edit form — `title` editable only on create (rename = git-side). */
function WikiEditForm(props: {
  owner: string;
  repoName: string;
  mode: "create" | "edit";
  initialTitle: string;
  initialContent: string;
}) @{
  const owner = props.owner;
  const repoName = props.repoName;
  const base = `/${owner}/${repoName}/wiki`;
  const [title, setTitle] = useState(props.initialTitle);
  const [content, setContent] = useState(props.initialContent);
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);

  async function deletePage() {
    setError("");
    setDeleting(true);
    try {
      const res = await apiClient.wiki.pages.delete({
        owner,
        name: repoName,
        page: props.initialTitle,
      });
      if (!res.ok) {
        setError(res.error.message || "Could not delete page.");
        setDeleting(false);
        setDeleteOpen(false);
        return;
      }
      toastSuccess("Page deleted.");
      navigate(base);
    } catch {
      setError(NETWORK_ERROR);
      setDeleting(false);
      setDeleteOpen(false);
    }
  }

  async function submit() {
    setError("");
    const cleanTitle = title.trim();
    if (props.mode === "create" && !cleanTitle) {
      setError("Page title is required.");
      return;
    }
    setPending(true);
    try {
      const res =
        props.mode === "create"
          ? await apiClient.wiki.pages.create({
              owner,
              name: repoName,
              title: cleanTitle,
              content,
              ...(message.trim() ? { message: message.trim() } : {}),
            })
          : await apiClient.wiki.pages.update({
              owner,
              name: repoName,
              page: props.initialTitle,
              content,
              ...(message.trim() ? { message: message.trim() } : {}),
            });
      if (!res.ok) {
        setError(res.error.message || "Could not save page.");
        setPending(false);
        return;
      }
      toastSuccess(props.mode === "create" ? "Page created." : "Page updated.");
      navigate(pageHref(owner, repoName, res.data.page.page_name));
    } catch {
      setError(NETWORK_ERROR);
      setPending(false);
    }
  }

  <>
    <form
      method="post"
      action="#"
      className="flex flex-col gap-4"
      data-testid="wiki-edit-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      @if (props.mode === "create") {
        <div className="flex flex-col gap-2">
          <Label htmlFor="wiki-page-title">Page title</Label>
          <Input
            id="wiki-page-title"
            value={title}
            onInput={(e) => setTitle((e.currentTarget as HTMLInputElement).value)}
            placeholder="Home"
            disabled={pending}
            required
          />
          <p className="text-[13px] text-muted-foreground">
            Spaces become dashes, GitHub-style — "My Page" saves as My-Page.
          </p>
        </div>;
      } @else {
        <h2 className="text-[18px] font-semibold text-foreground">
          {("Editing " + props.initialTitle) as string}
        </h2>;
      }
      <MarkdownWritePreview
        id="wiki-page-content"
        value={content}
        onValueChange={setContent}
        owner={owner}
        repo={repoName}
        rows={16}
        disabled={pending}
        placeholder="Write page content in Markdown…"
      />
      <div className="flex flex-col gap-2">
        <Label htmlFor="wiki-edit-message">Edit message (optional)</Label>
        <Input
          id="wiki-edit-message"
          value={message}
          onInput={(e) => setMessage((e.currentTarget as HTMLInputElement).value)}
          placeholder={props.mode === "create" ? "Created page" : "Updated page"}
          disabled={pending}
        />
      </div>
      @if (error) {
        <p role="alert" className="text-[14px] text-destructive">
          {error as string}
        </p>;
      }
      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={pending}>
          {(pending ? "Saving…" : props.mode === "create" ? "Create page" : "Save page") as string}
        </Button>
        <AppLink
          href={props.mode === "edit" ? pageHref(owner, repoName, props.initialTitle) : base}
          className="inline-flex h-8 items-center rounded-md border border-border px-3 text-[13px] text-foreground no-underline hover:bg-muted"
        >
          Cancel
        </AppLink>
        @if (props.mode === "edit") {
          <Button
            type="button"
            variant="ghost"
            className="ml-auto text-destructive hover:text-destructive"
            disabled={pending || deleting}
            onClick={() => setDeleteOpen(true)}
            data-testid="wiki-delete-open"
          >
            <Trash2 aria-hidden size={14} />
            {(deleting ? "Deleting…" : "Delete page") as string}
          </Button>;
        }
      </div>
    </form>
    @if (props.mode === "edit") {
      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete page?</AlertDialogTitle>
            <AlertDialogDescription>
              {
                ("“" +
                  props.initialTitle +
                  "” will be removed from the wiki. The page stays recoverable in the wiki's git history.") as string
              }
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleting}>Cancel</AlertDialogCancel>
            <Button
              type="button"
              variant="destructive"
              disabled={deleting}
              onClick={() => {
                void deletePage();
              }}
              data-testid="wiki-delete-confirm"
            >
              Delete page
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>;
    }
  </>;
}

function WikiPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/wiki/$", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
    _splat?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const loaderData = useQuery(repoWikiQueryOptions(params, searchStr)).data;
  const phase: Phase = loaderData ?? { kind: "loading" };

  const base = `/${owner}/${repoName}/wiki`;

  @if (phase.kind === "not_found") {
    <RepoNotFound />;
  } @else if (phase.kind === "loading") {
    <p className="text-[16px] text-muted-foreground">Loading wiki…</p>;
  } @else if (phase.kind === "error") {
    <p role="alert" className="text-[16px] text-destructive">
      {phase.message as string}
    </p>;
  } @else if (phase.kind === "disabled") {
    <RepoUnitDisabled unit="wiki" canAdmin={phase.repo.can_admin} />;
  } @else {
    <div className="flex flex-col gap-6 lg:flex-row">
      <div className="min-w-0 flex-1">
        @if (phase.headerHtml) {
          <MarkdownHtml
            html={phase.headerHtml ?? ""}
            className="mb-4 border-b border-border pb-3"
            data-testid="wiki-custom-header"
          />;
        }
        @if (
          phase.view === "new" ||
          (phase.view === "edit" && phase.pageMissing)
        ) {
          @if (phase.repo.can_write) {
            <WikiEditForm
              owner={owner}
              repoName={repoName}
              mode="create"
              initialTitle={phase.view === "edit" ? phase.pageName : ""}
              initialContent=""
            />;
          } @else {
            <EmptyState
              icon={<BookOpen aria-hidden size={22} />}
              title="Only people with write access can create wiki pages"
              description="Ask a repository maintainer to add the first page."
            />;
          }
        } @else if (phase.view === "edit") {
          <WikiEditForm
            owner={owner}
            repoName={repoName}
            mode="edit"
            initialTitle={phase.pageName}
            initialContent={phase.page?.content ?? ""}
          />;
        } @else if (phase.view === "history") {
          <div className="flex flex-col gap-3" data-testid="wiki-history">
            <div className="flex items-center gap-3">
              <h2 className="text-[18px] font-semibold text-foreground">
                {("Revision history — " + phase.pageName) as string}
              </h2>
              <AppLink
                href={pageHref(owner, repoName, phase.pageName)}
                className="text-[13px] text-primary no-underline hover:underline"
              >
                Back to page
              </AppLink>
            </div>
            @if (phase.revisions.length === 0) {
              <p className="text-[14px] text-muted-foreground">No revisions yet.</p>;
            } @else {
              <ul className="flex flex-col divide-y divide-border rounded-md border border-border">
                @for (const rev of phase.revisions; key rev.sha) {
                  <li className="flex flex-wrap items-baseline gap-x-3 px-3 py-2">
                    <code className="text-[12px] text-muted-foreground">
                      {rev.sha.slice(0, 7) as string}
                    </code>
                    <span className="text-[14px] text-foreground">
                      {rev.message as string}
                    </span>
                    <span className="ml-auto text-[13px] text-muted-foreground">
                      {
                        ((rev.author_username ?? rev.author_name) +
                          " · " +
                          formatRelative(rev.timestamp)) as string
                      }
                    </span>
                  </li>;
                }
              </ul>;
            }
          </div>;
        } @else if (phase.pageMissing && phase.view === "page") {
          <EmptyState
            icon={<BookOpen aria-hidden size={22} />}
            title={("“" + phase.pageName + "” does not exist yet") as string}
            description="This wiki page hasn't been created."
            action={
              phase.repo.can_write ? (
                <AppLink
                  href={`${pageHref(owner, repoName, phase.pageName)}/_edit`}
                  className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-[13px] font-medium text-primary-foreground no-underline hover:bg-primary/90"
                >
                  <FilePlus2 aria-hidden size={14} />
                  Create this page
                </AppLink>
              ) : undefined
            }
          />;
        } @else if (phase.page) {
          <>
            <div className="mb-4 flex flex-wrap items-center gap-3 border-b border-border pb-3">
              <h2 className="text-[20px] font-semibold text-foreground">
                {phase.page.page_name as string}
              </h2>
              @if (phase.repo.can_write) {
                <div className="ml-auto flex gap-2">
                  <AppLink
                    href={`${pageHref(owner, repoName, phase.pageName || phase.page.page_name)}/_edit`}
                    className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-3 text-[13px] text-foreground no-underline hover:bg-muted"
                  >
                    <Pencil aria-hidden size={14} />
                    Edit
                  </AppLink>
                  <AppLink
                    href={`${pageHref(owner, repoName, phase.pageName || phase.page.page_name)}/_history`}
                    className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-3 text-[13px] text-foreground no-underline hover:bg-muted"
                  >
                    <History aria-hidden size={14} />
                    History
                  </AppLink>
                </div>;
              }
            </div>
            @if (phase.markdownHtml) {
              <MarkdownHtml
                html={phase.markdownHtml ?? ""}
                data-testid="wiki-page-body"
              />;
            } @else {
              <MarkdownBody
                markdown={phase.page.content ?? ""}
                owner={owner}
                repo={repoName}
                data-testid="wiki-page-body"
              />;
            }
          </>;
        } @else {
          <EmptyState
            icon={<BookOpen aria-hidden size={22} />}
            title="Welcome to the wiki"
            description="Wikis keep documentation next to the code. Create the first page to get started — or push to the wiki git remote directly."
            action={
              phase.repo.can_write ? (
                <AppLink
                  href={`${base}/_new`}
                  className="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-[13px] font-medium text-primary-foreground no-underline hover:bg-primary/90"
                >
                  <FilePlus2 aria-hidden size={14} />
                  Create the first page
                </AppLink>
              ) : undefined
            }
          />;
        }
        @if (phase.footerHtml) {
          <MarkdownHtml
            html={phase.footerHtml ?? ""}
            className="mt-6 border-t border-border pt-3 text-[13px] text-muted-foreground"
            data-testid="wiki-custom-footer"
          />;
        }
      </div>
      <WikiSidebar
        owner={owner}
        repoName={repoName}
        pages={phase.pages}
        sidebarHtml={phase.sidebarHtml}
        canWrite={!!phase.repo.can_write}
        currentPage={phase.pageName || (phase.page?.page_name ?? "")}
      />
    </div>;
  }
}

/** Wiki — GitHub-style `{name}.wiki.git` backed pages (COL-13 Wiki unit). */
export function RepoWikiPage() @{
  <AppPage>
    <RepoLayout>
      <WikiPageContent />
    </RepoLayout>
  </AppPage>;
}
