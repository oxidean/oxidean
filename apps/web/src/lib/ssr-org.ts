import { type OrgMineEntry, type OrgPublic, type RepoPublic } from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";
import { fetchOrgProfileReadme, type ProfileReadme } from "@/lib/profile-readme";

export type OrgOverviewPayload = {
  org: OrgPublic;
  memberCount: number | null;
  repos: RepoPublic[];
  canAdmin: boolean;
  /** Public `.oxidean` / `.github` `profile/README.md`, or null. */
  profileReadme: ProfileReadme | null;
};

/** Org overview — org.get + member count + ACL-filtered repos (D-ORG-06). */
export async function fetchOrgOverview(data: {
  owner: string;
}): Promise<OrgOverviewPayload | null> {
  const slug = (data.owner ?? "").trim();
  if (!slug) return null;

  const got = await apiClient.org.get({ slug });
  if (!got.ok) return null;

  const org = got.data;
  let memberCount: number | null = null;
  let canAdmin = false;

  const mine = await apiClient.org.listMine();
  if (mine.ok) {
    const entry: OrgMineEntry | undefined = mine.data.orgs.find((o) => o.slug === org.slug);
    if (entry) {
      canAdmin = entry.role === "owner" || entry.role === "admin";
    }
  }

  const members = await apiClient.org.members.list({ slug: org.slug });
  if (members.ok) {
    memberCount = members.data.members.length;
  }

  let repos: RepoPublic[] = [];
  const listed = await apiClient.repo.listByOwner({ owner: org.slug });
  if (listed.ok) {
    repos = listed.data.repos;
  }

  const profileReadme = await fetchOrgProfileReadme(apiClient, org.slug);

  return { org, memberCount, repos, canAdmin, profileReadme };
}

/** org.get for settings loaders. */
export async function fetchOrgGet(data: { slug: string }) {
  return apiClient.org.get({ slug: (data.slug ?? "").trim() });
}

/** org.listMine for Admin+ gates. */
export async function fetchOrgListMine() {
  return apiClient.org.listMine();
}
