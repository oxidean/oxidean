import { type PublicUserProfile, type RepoPublic } from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";
import { fetchUserProfileReadme, type ProfileReadme } from "@/lib/profile-readme";

export type UserProfilePayload = {
  profile: PublicUserProfile;
  repos: RepoPublic[];
  starred: RepoPublic[];
  isSelf: boolean;
  /** Public `username/username` root README, or null. */
  profileReadme: ProfileReadme | null;
};

/** Public user profile + ACL-filtered repos (D-SOC-05…08). */
export async function fetchUserProfile(data: {
  username: string;
}): Promise<UserProfilePayload | null> {
  const username = (data.username ?? "").trim();
  if (!username) return null;

  const got = await apiClient.user.getPublicProfile({ username });
  if (!got.ok) return null;

  let repos: RepoPublic[] = [];
  const listed = await apiClient.repo.listByOwner({ owner: username });
  if (listed.ok) {
    repos = listed.data.repos;
  }

  let starred: RepoPublic[] = [];
  let isSelf = false;
  const me = await apiClient.auth.me();
  if (me.ok && me.data && me.data.username === username) {
    isSelf = true;
    const stars = await apiClient.user.listStarred({ offset: 0, limit: 30 });
    if (stars.ok) {
      starred = stars.data.repos;
    }
  }

  const profileReadme = await fetchUserProfileReadme(apiClient, username);

  return { profile: got.data, repos, starred, isSelf, profileReadme };
}
