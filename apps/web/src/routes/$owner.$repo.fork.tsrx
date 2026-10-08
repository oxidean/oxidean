import { useState } from "octane";
import { queryOptions, useQuery } from "@octanejs/tanstack-query";

import { matchPath } from "@/lib/route-params";
import { usePathname, useSearchString } from "@/lib/pathname.tsrx";
import { RepoLayout } from "@/routes/$owner.$repo.tsrx";
import { AppPage } from "@/components/app-page";
import type { RepoPublic, UserPublic } from "@oxidean/api-client";
import { AuthErrorBanner, AuthShell } from "@/components/auth-shell";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiClient } from "@/lib/api-client";
import { usePageRedirect } from "@/lib/nav.tsrx";
import { fetchSessionMe } from "@/lib/ssr-auth";
import { fetchRepoGet } from "@/lib/ssr-repo";

export type ForkLoaderData =
  | { kind: "ok"; user: UserPublic; source: RepoPublic }
  | { kind: "redirect"; href: string };

function repoForkQueryOptions(params: Record<string, string | undefined>, searchStr = "") {
  return queryOptions({
    queryKey: [
      "repo-page",
      "/$owner/$repo/fork",
      params.owner ?? "",
      params.repo ?? "",
      params._splat ?? "",
      params.n ?? "",
      params.sha ?? "",
      params.tag ?? "",
      params.run ?? "",
      searchStr,
    ],
    queryFn: async (): Promise<ForkLoaderData> => {
      const owner = String((params as { owner?: string }).owner ?? "");
      const repoName = String((params as { repo?: string }).repo ?? "");
      const me = await fetchSessionMe();
      if (!me.ok) {
        return {
          kind: "redirect",
          href: `/login?returnTo=${encodeURIComponent(`/${owner}/${repoName}/fork`)}`,
        };
      }
      // Cookie-forward SSR — never browser apiClient (same NETWORK_ERROR trap as PRs).
      const got = await fetchRepoGet({ owner, name: repoName });
      if (!got.ok || !got.data || got.data.visibility === "private") {
        return { kind: "redirect", href: `/${owner}/${repoName}` };
      }
      return { kind: "ok", user: me.data, source: got.data };
    },
    enabled: Boolean(params.owner && params.repo),
    retry: false,
    staleTime: 30_000,
  });
}

function ForkConfirmPageContent() @{
  const pathname = usePathname();
  const searchStr = useSearchString();
  const params = (matchPath("/$owner/$repo/fork", pathname) ?? {}) as {
    owner?: string;
    repo?: string;
  };
  const data = useQuery(repoForkQueryOptions(params, searchStr)).data;
  usePageRedirect(data?.kind === "redirect" ? data.href : null);
  const source = data?.kind === "ok" ? data.source : undefined;
  const user = data?.kind === "ok" ? data.user : undefined;
  const [name, setName] = useState(source?.name ?? params.repo ?? "");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

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
      window.location.assign(`/${dest.owner_username}/${dest.name}`);
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

/** Page root — RepoLayout (chrome + repo store) around the leaf content. */
export function ForkConfirmPage() @{
  <AppPage>
    <RepoLayout>
      <ForkConfirmPageContent />
    </RepoLayout>
  </AppPage>;
}
