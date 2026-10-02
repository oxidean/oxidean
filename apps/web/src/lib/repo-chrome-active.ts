/** Chrome tab highlight derived from a repo pathname (D-QH-01). */
export type RepoChromeActive =
  | "code"
  | "commits"
  | "branches"
  | "tags"
  | "issues"
  | "pulls"
  | "releases"
  | "packages"
  | "actions"
  | "insights"
  | "settings"
  | "search";

const SEGMENT_TO_ACTIVE: Record<string, RepoChromeActive> = {
  issues: "issues",
  pulls: "pulls",
  pull: "pulls",
  releases: "releases",
  packages: "packages",
  actions: "actions",
  insights: "insights",
  settings: "settings",
  commits: "commits",
  branches: "branches",
  tags: "tags",
  search: "search",
};

/**
 * Map `/{owner}/{repo}/…` pathname → RepoChrome `active` tab.
 * Known first segments under the repo highlight that tab; everything else is Code.
 */
export function repoChromeActiveFromPath(pathname: string): RepoChromeActive {
  const noQuery = (pathname.split("?")[0] ?? "/").split("#")[0] ?? "/";
  const parts = noQuery.split("/").filter(Boolean);
  // [owner, repo, segment?, ...]
  const segment = (parts[2] ?? "").toLowerCase();
  return SEGMENT_TO_ACTIVE[segment] ?? "code";
}
