import { createFileRoute, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useQuery, useQueryClient } from "@octanejs/tanstack-query";
import type { LabelPublic, RepoPublic } from "@oxidean/api-client";
import { useForm } from "@octanejs/tanstack-form";
import { useState } from "octane";
import { AuthErrorBanner } from "@/components/auth-shell";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import type { RepoLayoutLoaderData } from "@/lib/repo-store";
import { fetchRepoGet } from "@/lib/ssr-repo";
import { AppLink } from "@/components/ui/app-link";

export type RepoLabelsLoaderData =
  | { kind: "not_found" }
  | { kind: "error"; message: string }
  | { kind: "ready"; repo: RepoPublic };

const NETWORK_ERROR = "Can't reach Oxidean. Check your connection and try again.";

function labelsQueryKey(owner: string, name: string) {
  return ["label", "listForRepo", owner, name, "admin"] as const;
}

function swatchStyle(color: string): { backgroundColor: string } {
  const hex = color.startsWith("#") ? color : `#${color}`;
  return { backgroundColor: hex };
}

async function loadRepoLabels(owner: string, repoName: string): Promise<RepoLabelsLoaderData> {
  const got = await fetchRepoGet({ data: { owner, name: repoName } });
  if (!got.ok) {
    if (got.error.code === "repo.not_found") return { kind: "not_found" };
    return {
      kind: "error",
      message: got.error.message || "Could not load repository.",
    };
  }
  if (!got.data.can_admin) return { kind: "not_found" };
  return { kind: "ready", repo: got.data };
}

export const Route = createFileRoute("/$owner/$repo/issues/labels")({
  component: RepoIssueLabelsPage,
  head: ({ params }) => ({
    meta: [
      {
        title: `Labels · ${(params as { owner: string }).owner}/${(params as { repo: string }).repo} · Oxidean`,
      },
    ],
  }),
  loader: async ({ params }): Promise<RepoLabelsLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    try {
      return await loadRepoLabels(owner, repoName);
    } catch {
      return { kind: "error", message: NETWORK_ERROR };
    }
  },
});

/** Repo label overrides — local labels + hide org defaults (D-ISS-05 / D-ISS-07). */
export function RepoIssueLabelsPage() @{
  const params = useParams({ strict: false }) as {
    owner?: string;
    repo?: string;
  };
  const owner = params.owner ?? "";
  const repoName = params.repo ?? "";
  const layout = useLoaderData({ from: "/$owner/$repo" }) as RepoLayoutLoaderData | undefined;
  const routeLoader = useLoaderData({
    from: "/$owner/$repo/issues/labels",
  }) as RepoLabelsLoaderData | undefined;
  const queryClient = useQueryClient();

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

  const labelsQ = useQuery({
    queryKey: labelsQueryKey(owner, repoName),
    queryFn: async () => {
      const res = await apiClient.label.listForRepo({
        owner,
        name: repoName,
        includeHidden: true,
      });
      if (!res.ok) throw new Error(res.error.message || NETWORK_ERROR);
      return res.data.labels;
    },
    enabled: !!owner && !!repoName && !!repoQ.data?.can_admin,
  });

  const [formError, setFormError] = useState("");
  const [pending, setPending] = useState(false);
  const [rowError, setRowError] = useState("");

  const form = useForm({
    defaultValues: {
      name: "",
      color: "5319e7",
      description: "",
    },
    onSubmit: async ({ value }) => {
      setFormError("");
      const trimmed = value.name.trim();
      if (!trimmed) {
        setFormError("Name is required.");
        return;
      }
      setPending(true);
      try {
        const res = await apiClient.label.create({
          scope: "repo",
          owner,
          repo: repoName,
          name: trimmed,
          color: value.color,
          description: value.description.trim() || null,
        });
        if (!res.ok) {
          setFormError(res.error.message || NETWORK_ERROR);
          setPending(false);
          return;
        }
        form.reset();
        setPending(false);
        await refresh();
      } catch {
        setFormError(NETWORK_ERROR);
        setPending(false);
      }
    },
  });

  const repo = repoQ.data ?? null;
  const canAdmin = !!repo?.can_admin;
  const labels: LabelPublic[] = labelsQ.data ?? [];
  const notFound =
    layout?.status === "not_found" ||
    routeLoader?.kind === "not_found" ||
    (!repoQ.isPending && !!repo && !canAdmin) ||
    (!repoQ.isPending && !repo);

  async function refresh() {
    await queryClient.invalidateQueries({
      queryKey: labelsQueryKey(owner, repoName),
    });
    await queryClient.invalidateQueries({
      queryKey: ["label", "listForRepo", owner, repoName],
    });
  }

  async function toggleHidden(lab: LabelPublic) {
    if (lab.scope !== "org") return;
    setRowError("");
    try {
      const res = await apiClient.label.update({
        id: lab.id,
        owner,
        repo: repoName,
        hidden: !lab.hidden,
      });
      if (!res.ok) {
        setRowError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refresh();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  async function deleteLocal(lab: LabelPublic) {
    if (lab.scope !== "repo") return;
    setRowError("");
    try {
      const res = await apiClient.label.delete({
        id: lab.id,
        owner,
        repo: repoName,
      });
      if (!res.ok) {
        setRowError(res.error.message || NETWORK_ERROR);
        return;
      }
      await refresh();
    } catch {
      setRowError(NETWORK_ERROR);
    }
  }

  @if (notFound) {
    <RepoNotFound />;
  } @else {
    @if (!repo) {
      <p className="text-[16px] text-muted-foreground">Loading labels…</p>;
    } @else {
      <>
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-[18px] font-semibold text-foreground">Labels</h2>
            <p className="text-[14px] text-muted-foreground">
              Hide org defaults or add repository-local labels.
            </p>
          </div>
          <AppLink
            href={`/${owner}/${repoName}/issues` as string}
            className="text-[14px] text-primary no-underline hover:underline"
          >
            Back to issues
          </AppLink>
        </div>

        @if (formError) {
          <div className="mb-4">
            <AuthErrorBanner message={formError} />
          </div>;
        }
        @if (rowError) {
          <div className="mb-4">
            <AuthErrorBanner message={rowError} />
          </div>;
        }

        <form
          method="post"
          action="#"
          className="mb-8 flex flex-col gap-3 rounded-md border border-border p-4"
          onSubmit={(e) => {
            e.preventDefault();
            void form.handleSubmit();
          }}
        >
          <p className="text-[14px] font-semibold text-foreground">New repository label</p>
          <div className="grid gap-3 sm:grid-cols-2">
            <form.Field name="name">
              {(field) => (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="repo-label-name">Name</Label>
                  <Input
                    id="repo-label-name"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                    disabled={pending}
                  />
                </div>
              )}
            </form.Field>
            <form.Field name="color">
              {(field) => (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="repo-label-color">Color (hex)</Label>
                  <Input
                    id="repo-label-color"
                    value={field.state.value}
                    onBlur={field.handleBlur}
                    onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                    disabled={pending}
                  />
                </div>
              )}
            </form.Field>
          </div>
          <form.Field name="description">
            {(field) => (
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="repo-label-desc">Description</Label>
                <Input
                  id="repo-label-desc"
                  value={field.state.value}
                  onBlur={field.handleBlur}
                  onInput={(e) => field.handleChange((e.target as HTMLInputElement).value)}
                  disabled={pending}
                />
              </div>
            )}
          </form.Field>
          <div>
            <Button type="submit" disabled={pending}>
              Create label
            </Button>
          </div>
        </form>

        @if (labelsQ.isPending) {
          <p className="text-[16px] text-muted-foreground">Loading labels…</p>;
        } @else {
          @if (labels.length === 0) {
            <p className="text-[16px] text-muted-foreground">
              No labels yet. Create a local label or add org defaults.
            </p>;
          } @else {
            <ul className="divide-y divide-border border-y border-border">
              @for (const lab of labels; key lab.id) {
                <li className="flex flex-wrap items-center justify-between gap-3 py-3">
                  <div className="flex flex-wrap items-center gap-2">
                    <span
                      className="inline-block size-3 rounded-sm border border-border"
                      style={swatchStyle(lab.color)}
                      aria-hidden
                    />
                    <span className="text-[15px] font-medium text-foreground">
                      {lab.name as string}
                    </span>
                    <span className="text-[12px] text-muted-foreground">
                      {(lab.scope === "org" ? "org" : "repo") as string}
                    </span>
                    @if (lab.hidden) {
                      <span className="text-[12px] text-muted-foreground">
                        hidden
                      </span>;
                    }
                  </div>
                  <div className="flex flex-wrap gap-2">
                    @if (lab.scope === "org") {
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => toggleHidden(lab)}
                      >
                        {(lab.hidden ? "Show" : "Hide") as string}
                      </Button>;
                    }
                    @if (lab.scope === "repo") {
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => deleteLocal(lab)}
                      >
                        Delete
                      </Button>;
                    }
                  </div>
                </li>;
              }
            </ul>;
          }
        }
      </>;
    }
  }
}

export default RepoIssueLabelsPage;
