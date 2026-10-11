import { queryOptions, type QueryClient } from "@octanejs/tanstack-query";
import type {
  AuthSettingsPublic,
  BootstrapStatus,
  OrgMineEntry,
  ProviderConfigPublic,
  RepoPublic,
  UserPublic,
} from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";

export const authMeQueryKey = ["auth", "me"] as const;
const authBootstrapQueryKey = ["auth", "bootstrapStatus"] as const;
export const authProviderConfigQueryKey = ["auth", "providerConfig"] as const;
export const adminAuthSettingsQueryKey = ["admin", "auth", "getSettings"] as const;
const orgListMineQueryKey = ["org", "listMine"] as const;

/** Soft session read — unauthenticated / pre-setup → `null` (shared chrome / banner cache). */
export function authSessionQueryOptions() {
  return queryOptions({
    queryKey: authMeQueryKey,
    queryFn: async (): Promise<UserPublic | null> => {
      const res = await apiClient.auth.me();
      if (!res.ok) {
        // Expected anonymous / lock states — never throw (throws → Query remount refetch spam).
        if (res.error.code === "auth.unauthenticated" || res.error.code === "auth.setup_required") {
          return null;
        }
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    retry: false,
    staleTime: 30_000,
  });
}

export function authBootstrapQueryOptions() {
  return queryOptions({
    queryKey: authBootstrapQueryKey,
    queryFn: async (): Promise<BootstrapStatus> => {
      const res = await apiClient.auth.bootstrapStatus();
      if (!res.ok) {
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    retry: false,
    staleTime: 30_000,
  });
}

/** Fail-closed signup flag when locked or anonymous config unavailable. */
const PROVIDER_CONFIG_LOCKED: ProviderConfigPublic = {
  mode: "local",
  allow_signup: false,
};

export function authProviderConfigQueryOptions() {
  return queryOptions({
    queryKey: authProviderConfigQueryKey,
    queryFn: async (): Promise<ProviderConfigPublic> => {
      const res = await apiClient.auth.providerConfig();
      if (!res.ok) {
        if (res.error.code === "auth.setup_required") {
          return PROVIDER_CONFIG_LOCKED;
        }
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data;
    },
    retry: false,
    staleTime: 30_000,
  });
}

export function adminAuthSettingsQueryOptions() {
  return queryOptions({
    queryKey: adminAuthSettingsQueryKey,
    queryFn: async (): Promise<AuthSettingsPublic> => {
      const res = await apiClient.admin.auth.getSettings();
      if (!res.ok) {
        const err = new Error(`${res.error.code}: ${res.error.message}`) as Error & {
          code: string;
        };
        err.code = res.error.code;
        throw err;
      }
      return res.data;
    },
  });
}

/** Soft org memberships for chrome account menu — empty on auth failures. */
export function orgListMineQueryOptions() {
  return queryOptions({
    queryKey: orgListMineQueryKey,
    queryFn: async (): Promise<OrgMineEntry[]> => {
      const res = await apiClient.org.listMine();
      if (!res.ok) {
        if (res.error.code === "auth.unauthenticated" || res.error.code === "auth.setup_required") {
          return [];
        }
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data.orgs;
    },
    retry: false,
    staleTime: 30_000,
  });
}

const repoListMineQueryKey = ["repo", "listMine"] as const;

/** Signed-in home repo list — empty on auth failures (matches orgListMine). */
export function repoListMineQueryOptions() {
  return queryOptions({
    queryKey: repoListMineQueryKey,
    queryFn: async (): Promise<RepoPublic[]> => {
      const res = await apiClient.repo.listMine();
      if (!res.ok) {
        if (res.error.code === "auth.unauthenticated" || res.error.code === "auth.setup_required") {
          return [];
        }
        throw new Error(`${res.error.code}: ${res.error.message}`);
      }
      return res.data.repos;
    },
    retry: false,
    staleTime: 30_000,
  });
}

/** After logout / factory reset — drop session and related auth caches. */
export function clearSessionQueries(qc: QueryClient) {
  qc.setQueryData(authMeQueryKey, null);
  qc.setQueryData(orgListMineQueryKey, []);
  void qc.invalidateQueries({ queryKey: ["auth"] });
  void qc.invalidateQueries({ queryKey: ["admin"] });
  void qc.invalidateQueries({ queryKey: ["org"] });
  void qc.invalidateQueries({ queryKey: ["notification"] });
}

/** Keep chrome in sync after profile/avatar updates without a full reload. */
export function setAuthMeCache(qc: QueryClient, user: UserPublic | null) {
  qc.setQueryData(authMeQueryKey, user);
}

export function setAdminAuthSettingsCache(qc: QueryClient, settings: AuthSettingsPublic) {
  qc.setQueryData(adminAuthSettingsQueryKey, settings);
  void qc.invalidateQueries({ queryKey: authProviderConfigQueryKey });
}
