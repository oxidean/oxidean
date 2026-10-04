import { treeHref, pathBreadcrumbCrumbs } from "@/lib/repo-browse";
import { AppLink } from "@/components/ui/app-link";

type PathBreadcrumbProps = {
  owner: string;
  repo: string;
  refName: string;
  path: string;
};

/** Label-size path crumbs — repo → code home; segments → tree (D-17).
 * Long segments truncate with `title`; deep paths wrap (UI-SPEC long-path). */
export function PathBreadcrumb({ owner, repo, refName, path }: PathBreadcrumbProps) @{
  const crumbs = pathBreadcrumbCrumbs(path);
  const homeHref = `/${owner}/${repo}/`;

  <nav
    aria-label="Path"
    className="flex min-w-0 max-w-full flex-wrap items-center gap-1 text-[14px]"
  >
    <AppLink
      href={homeHref as string}
      className="min-w-0 max-w-[min(100%,12rem)] truncate font-semibold text-primary no-underline hover:underline"
      title={repo as string}
    >
      {repo as string}
    </AppLink>
    @for (const c of crumbs; key `${c.prefix}`) {
      const href = treeHref(owner, repo, refName, c.prefix);
      <span className="inline-flex min-w-0 items-center gap-1">
        <span className="shrink-0 text-muted-foreground" aria-hidden>
          /
        </span>
        @if (c.last) {
          <span
            className="min-w-0 max-w-[min(100%,16rem)] truncate font-semibold text-foreground"
            title={c.seg as string}
          >
            {c.seg as string}
          </span>;
        } @else {
          <AppLink
            href={href as string}
            className="min-w-0 max-w-[min(100%,12rem)] truncate font-normal text-primary no-underline hover:underline"
            title={c.seg as string}
          >
            {c.seg as string}
          </AppLink>;
        }
      </span>;
    }
  </nav>;
}
