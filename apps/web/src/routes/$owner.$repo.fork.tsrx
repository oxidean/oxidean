import { createFileRoute, redirect, useLoaderData, useParams } from "@octanejs/tanstack-router";
import { useState } from "octane";
import type { RepoPublic, UserPublic } from "@oxidean/api-client";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { useAppNavigate } from "@/lib/app-navigate";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { fetchRepoGet } from "@/lib/ssr-repo";

export type ForkLoaderData = {
  user: UserPublic;
  source: RepoPublic;
};

export const Route = createFileRoute("/$owner/$repo/fork")({
  component: ForkConfirmPage,
  head: ({ params }) => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repo = String((params as { repo?: string }).repo ?? "");
    return { meta: [{ title: `Fork ${owner}/${repo} · Oxidean` }] };
  },
  beforeLoad: async ({ params }) => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repo = String((params as { repo?: string }).repo ?? "");
    const returnTo = `/${owner}/${repo}/fork`;
    try {
      const me = await fetchSessionMe();
      if (!me.ok) {
        throw redirect({ href: `/login?returnTo=${encodeURIComponent(returnTo)}` });
      }
    } catch (e) {
      if (e && typeof e === "object" && "href" in e) throw e;
      throw redirect({ href: `/login?returnTo=${encodeURIComponent(returnTo)}` });
    }
  },
  loader: async ({ params }): Promise<ForkLoaderData> => {
    const owner = String((params as { owner?: string }).owner ?? "");
    const repoName = String((params as { repo?: string }).repo ?? "");
    const me = await fetchSessionMe();
    if (!me.ok) {
      throw redirect({
        href: `/login?returnTo=${encodeURIComponent(`/${owner}/${repoName}/fork`)}`,
      });
    }
    // Cookie-forward SSR — never browser apiClient (same NETWORK_ERROR trap as PRs).
    const got = await fetchRepoGet({ data: { owner, name: repoName } });
    if (!got.ok || !got.data) {
      throw redirect({ href: `/${owner}/${repoName}` });
    }
    if (got.data.visibility === "private") {
      throw redirect({ href: `/${owner}/${repoName}` });
    }
    return { user: me.data, source: got.data };
  },
});

export function ForkConfirmPage() @{
  const params = useParams({ from: "/$owner/$repo/fork" }) as {
    owner?: string;
    repo?: string;
  };
  const data = useLoaderData({ from: "/$owner/$repo/fork" }) as ForkLoaderData | undefined;
  const source = data?.source;
  const user = data?.user;
  const [name, setName] = useState(source?.name ?? params.repo ?? "");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const navigateApp = useAppNavigate();

  const sourceLabel = source
    ? `${source.owner_username}/${source.name}`
    : `${params.owner}/${params.repo}`;

  async function onFork() {
    if (!source || !user || pending) return;
    setPending(true);
    setError(null);
    try {
      const res = await apiClient.repo.fork({
        owner: source.owner_username,
        name: source.name,
        into_name: name.trim() || source.name,
      });
      if (!res.ok) {
        const code = (res.error as { code?: string } | undefined)?.code;
        if (code === "repo.name_taken") {
          setError("A repository with this name already exists. Choose a different name.");
        } else if (code === "repo.fork_exists") {
          setError("You already have a fork of this repository.");
        } else {
          setError((res.error as { message?: string } | undefined)?.message || "Fork failed.");
        }
        return;
      }
      const dest = res.data;
      void navigateApp(`/${dest.owner_username}/${dest.name}`);
    } catch {
      setError("Can't reach Oxidean. Check your connection and try again.");
    } finally {
      setPending(false);
    }
  }

  <AuthShell title="Create a fork" subtitle={`Fork ${sourceLabel}`}>
    @if (error) {
      <AuthErrorBanner message={error as string} />;
    }
    <div className="flex flex-col gap-4">
      <p className="text-[14px] text-muted-foreground">
        Owner: <span className="font-medium text-foreground">{"@" + (user?.username ?? "")}</span>
      </p>
      <div className="flex flex-col gap-2">
        <Label htmlFor="fork-name">Repository name</Label>
        <Input
          id="fork-name"
          value={name as string}
          onInput={(e) => setName((e.currentTarget as HTMLInputElement).value)}
        />
      </div>
      <Button type="button" disabled={pending} onClick={() => void onFork()}>
        {(pending ? "Forking…" : "Create fork") as string}
      </Button>
    </div>
  </AuthShell>;
}
