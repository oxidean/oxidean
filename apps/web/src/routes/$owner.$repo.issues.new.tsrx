import { createFileRoute, useLoaderData, useNavigate, useParams } from "@octanejs/tanstack-router";
import { useForm } from "@octanejs/tanstack-form";
import { useQuery } from "@octanejs/tanstack-query";
import type { RepoPublic } from "@oxidean/api-client";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";
import { fetchRepoGet } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type IssuesNewLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "ready"; repo: RepoPublic }
  | { kind: "forbidden"; repo: RepoPublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

async function loadNewIssuePage(owner: string, repoName: string): Promise<IssuesNewLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  if (!got.data.can_write) {
    return { kind: "forbidden", repo: got.data };
  }
  return { kind: "ready", repo: got.data };
}

export const Route = createFileRoute("/$owner/$repo/issues/new")({
  component: IssuesNewPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `New issue · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<IssuesNewLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    try {
      return await loadNewIssuePage(owner, repoName);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Create issue form with Write | Preview (D-ISS-10 / D-ISS-19). */
export function IssuesNewPage() @{
  const params = useParams({ strict: false }) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const navigate = useNavigate();
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const routeLoader = useLoaderData({ from: "/$owner/$repo/issues/new" }) as
    | IssuesNewLoaderData
    | undefined;

  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");

  const form = useForm({
    defaultValues: {
      title: "",
      body: "",
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
        await navigate({
          to: "/$owner/$repo/issues/$n",
          params: {
            owner,
            repo: repoName,
            n: String(res.data.number),
          },
        });
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

  const repo = repoQ.data ?? null;

  @if (layout?.status === "not_found" || (!repoQ.isPending && !repo)) {
    <RepoNotFound />;
  } @else if (!repo) {
    <p className="text-[16px] text-muted-foreground">Loading…</p>;
  } @else if (!repo.can_write) {
    <p role="alert" className="text-[16px] text-destructive">
      You need write access to create issues.
    </p>;
  } @else {
    <>
      <h2 className="mb-4 text-[18px] font-semibold text-foreground">New issue</h2>
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
}

export default IssuesNewPage;
