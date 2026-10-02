import { createFileRoute, useNavigate, useParams, useSearch } from "@octanejs/tanstack-router";
import { useState } from "octane";
import { MemberLookup } from "@/components/org/member-lookup";
import { MarkdownWritePreview } from "@/components/repo/markdown-write-preview";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoUnitDisabled } from "@/components/repo/repo-unit-disabled";
import { apiClient } from "@/lib/api-client";
import { fetchRepoGet, fetchRepoRefs } from "@/lib/ssr-repo";

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

export const Route = createFileRoute("/$owner/$repo/pulls/new")({
  component: NewPullPage,
  validateSearch: (search: Record<string, unknown>) => ({
    base: typeof search.base === "string" ? search.base : undefined,
    head: typeof search.head === "string" ? search.head : undefined,
  }),
  head: ({ params }) => ({
    meta: [
      {
        title: `New pull request · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }) => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    const got = await fetchRepoGet({ data: { owner, name: repoName } });
    if (!got.ok) {
      if (got.error.code === "repo.not_found") return { kind: "not_found" as const };
      return { kind: "error" as const, message: got.error.message || NETWORK_ERROR };
    }
    // Unit toggle (COL-13): Pulls off → disabled view, skip ref enumeration.
    if (got.data.pulls_enabled === false) {
      return { kind: "disabled" as const, repo: got.data };
    }
    const refs = await fetchRepoRefs({ data: { owner, name: repoName } });
    return {
      kind: "ready" as const,
      repo: got.data,
      refs: refs.ok ? refs.data.refs : [],
    };
  },
});

export function NewPullPage() @{
  const params = useParams({ strict: false }) as {
    owner?: string;
    repo?: string;
  };
  const search = useSearch({ strict: false }) as { base?: string; head?: string };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const navigate = useNavigate();
  const loader = Route.useLoaderData() as
    | { kind: "not_found" }
    | { kind: "error"; message: string }
    | { kind: "disabled"; repo: { can_admin?: boolean } }
    | {
        kind: "ready";
        repo: { default_branch: string; can_write?: boolean };
        refs: { name: string }[];
      };

  const defaultBranch = loader.kind === "ready" ? loader.repo.default_branch || "main" : "main";
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [baseRef, setBaseRef] = useState(search.base || defaultBranch);
  const [headRef, setHeadRef] = useState(search.head || defaultBranch);
  const [headOwner, setHeadOwner] = useState(owner);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const branches =
    loader.kind === "ready"
      ? loader.refs
          .filter((r) => r.name.startsWith("refs/heads/"))
          .map((r) => r.name.replace(/^refs\/heads\//, ""))
      : [];

  async function onSubmit() {
    setBusy(true);
    setError(null);
    const headOwnerTrim = headOwner.trim() || owner;
    const res = await apiClient.pull.create({
      owner,
      name: repoName,
      title,
      body,
      base_ref: baseRef,
      head_ref: headRef,
      ...(headOwnerTrim !== owner ? { head_owner: headOwnerTrim } : {}),
    });
    setBusy(false);
    if (!res.ok) {
      setError(res.error.message || "Could not create pull request.");
      return;
    }
    void navigate({
      to: "/$owner/$repo/pull/$n",
      params: { owner, repo: repoName, n: String(res.data.number) },
    });
  }

  @if (loader.kind === "not_found") {
    <RepoNotFound />;
  } @else {
    @if (loader.kind === "disabled") {
      <RepoUnitDisabled unit="pulls" canAdmin={loader.repo.can_admin ?? false} />;
    } @else if (loader.kind === "error") {
      <p className="text-destructive">{loader.message as string}</p>;
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
            label="Head owner"
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
          For a fork head, set head owner to the fork owner (branch name must exist on that fork).
        </p>
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
      </div>;
    }
  }
}
