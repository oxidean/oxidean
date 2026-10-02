import type { OrgMineEntry, RepoPublic, UserLookupHit } from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";

export type SearchEntityType =
  | "repositories"
  | "users"
  | "organizations"
  | "code"
  | "issues"
  | "pulls";

/** Valid `type` values for `/search` — the same set the route's `parseType` accepts. */
export const SEARCH_ENTITY_TYPES: readonly SearchEntityType[] = [
  "repositories",
  "users",
  "organizations",
  "code",
  "issues",
  "pulls",
];

export type SearchRepoSuggestion = {
  kind: "repo";
  id: string;
  owner: string;
  name: string;
  visibility: string;
  description: string | null;
  href: string;
};

export type SearchUserSuggestion = {
  kind: "user";
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  href: string;
};

export type SearchOrgSuggestion = {
  kind: "org";
  slug: string;
  display_name: string | null;
  href: string;
};

export type SearchActionSuggestion = {
  kind: "action";
  type: SearchEntityType;
  label: string;
  href: string;
};

export type SearchSuggestions = {
  q: string;
  repositories: SearchRepoSuggestion[];
  users: SearchUserSuggestion[];
  organizations: SearchOrgSuggestion[];
  actions: SearchActionSuggestion[];
};

const REPO_LIMIT = 6;
const USER_LIMIT = 5;
const ORG_LIMIT = 5;

function matchesQ(hay: string, q: string): boolean {
  return hay.toLowerCase().includes(q.toLowerCase());
}

function repoToSuggestion(repo: RepoPublic): SearchRepoSuggestion {
  return {
    kind: "repo",
    id: repo.id,
    owner: repo.owner_username,
    name: repo.name,
    visibility: repo.visibility,
    description: repo.description ?? null,
    href: `/${repo.owner_username}/${repo.name}`,
  };
}

function mergeRepos(mine: RepoPublic[], explored: RepoPublic[], q: string): SearchRepoSuggestion[] {
  const byId = new Map<string, RepoPublic>();
  for (const r of [...mine, ...explored]) {
    if (!byId.has(r.id)) byId.set(r.id, r);
  }
  const all = [...byId.values()];
  const filtered = q
    ? all.filter((r) => {
        const key = `${r.owner_username}/${r.name} ${r.description ?? ""}`;
        return matchesQ(key, q);
      })
    : all;
  filtered.sort((a, b) => {
    const aPriv = a.visibility === "private" ? 0 : 1;
    const bPriv = b.visibility === "private" ? 0 : 1;
    if (aPriv !== bPriv) return aPriv - bPriv;
    return (b.updated_at ?? "").localeCompare(a.updated_at ?? "");
  });
  return filtered.slice(0, REPO_LIMIT).map(repoToSuggestion);
}

function filterOrgs(orgs: OrgMineEntry[], q: string): SearchOrgSuggestion[] {
  const filtered = q ? orgs.filter((o) => matchesQ(`${o.slug} ${o.display_name ?? ""}`, q)) : orgs;
  return filtered.slice(0, ORG_LIMIT).map((o) => ({
    kind: "org" as const,
    slug: o.slug,
    display_name: o.display_name ?? null,
    href: `/${o.slug}`,
  }));
}

function actionRows(q: string): SearchActionSuggestion[] {
  const encoded = encodeURIComponent(q);
  return [
    {
      kind: "action",
      type: "repositories",
      label: `Search repositories for “${q}”`,
      href: `/search?q=${encoded}&type=repositories`,
    },
    {
      kind: "action",
      type: "users",
      label: `Search users for “${q}”`,
      href: `/search?q=${encoded}&type=users`,
    },
    {
      kind: "action",
      type: "organizations",
      label: `Search organizations for “${q}”`,
      href: `/search?q=${encoded}&type=organizations`,
    },
    {
      kind: "action",
      type: "code",
      label: `Search code for “${q}”`,
      href: `/search?q=${encoded}&type=code`,
    },
    {
      kind: "action",
      type: "issues",
      label: `Search issues for “${q}”`,
      href: `/search?q=${encoded}&type=issues`,
    },
    {
      kind: "action",
      type: "pulls",
      label: `Search pull requests for “${q}”`,
      href: `/search?q=${encoded}&type=pulls`,
    },
  ];
}

/**
 * Omnibar suggestions: private + public repos you can access, users, orgs,
 * plus “Search … for q” action rows.
 */
export async function fetchSearchSuggestions(
  qRaw: string,
  opts?: { signedIn?: boolean },
): Promise<SearchSuggestions> {
  const q = qRaw.trim();
  const signedIn = opts?.signedIn === true;

  if (!q) {
    return { q: "", repositories: [], users: [], organizations: [], actions: [] };
  }

  const exploreP = apiClient.repo.explore({ q, offset: 0, limit: 20 });
  const mineP = signedIn
    ? apiClient.repo.listMine()
    : Promise.resolve({ ok: false as const, error: { code: "skip", message: "" } });
  const orgsP = signedIn
    ? apiClient.org.listMine()
    : Promise.resolve({ ok: false as const, error: { code: "skip", message: "" } });
  const usersP =
    signedIn && q.length >= 2 && !q.includes("@")
      ? apiClient.user.lookup({ prefix: q })
      : Promise.resolve({ ok: true as const, data: { users: [] as UserLookupHit[] } });

  const [exploreRes, mineRes, orgsRes, usersRes] = await Promise.all([
    exploreP,
    mineP,
    orgsP,
    usersP,
  ]);

  const explored = exploreRes.ok ? exploreRes.data.repos : [];
  const mine = mineRes.ok ? mineRes.data.repos : [];
  const orgs = orgsRes.ok ? orgsRes.data.orgs : [];
  const users: SearchUserSuggestion[] = usersRes.ok
    ? usersRes.data.users.slice(0, USER_LIMIT).map((u) => ({
        kind: "user" as const,
        username: u.username,
        display_name: u.display_name ?? null,
        avatar_url: u.avatar_url ?? null,
        href: `/${u.username}`,
      }))
    : [];

  return {
    q,
    repositories: mergeRepos(mine, explored, q),
    users,
    organizations: filterOrgs(orgs, q),
    actions: actionRows(q),
  };
}

export function searchResultsHref(q: string, type: SearchEntityType = "repositories"): string {
  const trimmed = q.trim();
  if (!trimmed) return `/search?type=${type}`;
  return `/search?q=${encodeURIComponent(trimmed)}&type=${type}`;
}
