import { useQuery } from "@octanejs/tanstack-query";
import type { UserPublic } from "@oxidean/api-client";
import { RepoChrome } from "@/components/repo/repo-chrome";
import { RepoNotFound } from "@/components/repo/repo-not-found";
import { RepoPageShell } from "@/components/repo/repo-page-shell";
import { RepoStoreProvider } from "@/components/repo/repo-store-provider";
import { Skeleton } from "@/components/ui/skeleton";
import { usePathname } from "@/lib/pathname.tsrx";
import { repoChromeActiveFromPath } from "@/lib/repo-chrome-active";
import { type RepoLayoutLoaderData, useRepoStore } from "@/lib/repo-store.tsrx";
import { repoLayoutQueryOptions } from "@/lib/repo-queries";
import {
  resolvePublicOriginClient,
  resolveSshAdvertiseHost,
  resolveSshAdvertisePort,
} from "@/lib/public-origin";
import { repoParamsFromPathname } from "@/lib/route-params";
import { authSessionQueryOptions } from "@/lib/session-queries";

export type { RepoLayoutLoaderData };

/**
 * Layout for `/{owner}/{repo}/*` — fixed max-w-7xl shell with repo/session
 * queries client-side (was: SSR cookie-forward loader). Every repo leaf page
 * wraps its content in `<RepoLayout>` — the store provider keeps loader-data
 * semantics alive for leaves (`useRepoStore`).
 */
export function RepoLayout(props: { children?: unknown }) @{
  const pathname = usePathname();
  const params = repoParamsFromPathname(pathname) ?? { owner: "", repo: "" };
  const owner = params.owner;
  const repoName = params.repo;

  const meQ = useQuery(authSessionQueryOptions());
  const repoQ = useQuery({
    ...repoLayoutQueryOptions(owner, repoName),
    enabled: Boolean(owner && repoName),
  });

  const publicOrigin = resolvePublicOriginClient();
  const sshHost = resolveSshAdvertiseHost(publicOrigin);
  const sshPort = resolveSshAdvertisePort();

  const status = repoQ.data?.status ?? (repoQ.isError ? "error" : null);
  const repo = repoQ.data?.repo ?? null;
  const message =
    repoQ.data?.message ??
    (repoQ.isError ? "Can't reach Oxidean. Check your connection and try again." : "");
  const me: UserPublic | null = meQ.data ?? null;

  @if (repoQ.isPending) {
    <RepoPageShell>
      <div className="flex flex-col gap-4" aria-busy="true">
        <Skeleton className="h-8 w-64" />
        <Skeleton className="h-10 w-full" />
        <Skeleton className="h-48 w-full" />
      </div>
    </RepoPageShell>;
  } @else if (status === "not_found") {
    <RepoNotFound />;
  } @else if (status === "error") {
    <RepoPageShell>
      <p role="alert" className="text-[16px] text-destructive">
        {message as string}
      </p>
    </RepoPageShell>;
  } @else {
    <RepoStoreProvider
      key={`${owner}/${repoName}` as string}
      owner={owner}
      repoName={repoName}
      status={status ?? "ok"}
      repo={repo}
      me={me}
      message={message}
      publicOrigin={publicOrigin}
      sshHost={sshHost}
      sshPort={sshPort}
    >
      <RepoPageShell>
        <RepoLayoutChrome>{props.children}</RepoLayoutChrome>
      </RepoPageShell>
    </RepoStoreProvider>;
  }
}

/** Shared repo chrome above leaf content (D-QH-01). */
function RepoLayoutChrome(props: { children?: unknown }) @{
  const repo = useRepoStore((s) => s.repo);
  const pathname = usePathname();
  const active = repoChromeActiveFromPath(pathname);

  @if (repo) {
    <>
      <RepoChrome repo={repo} active={active} />
      {props.children}
    </>;
  } @else {
    <>{props.children}</>;
  }
}
