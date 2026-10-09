import { type RepoPublic } from "@oxidean/api-client";
import { apiClient } from "@/lib/api-client";
import {
  resolveThemeForSsr,
  themePreferenceFromCookieHeader,
  resolvedColorSchemeFromCookieHeader,
} from "@/lib/theme";

/** auth.bootstrap_status. */
export async function fetchBootstrapStatus() {
  return apiClient.auth.bootstrapStatus();
}

/** auth.me — browser session cookie. */
export async function fetchSessionMe() {
  return apiClient.auth.me();
}

/** repo.createDefaults for /new visibility + catalogs (D-02, D-08). */
export async function fetchRepoCreateDefaults() {
  return apiClient.repo.createDefaults();
}

/** auth.provider_config (allow_signup). */

/** repo.listMine (signed-in home). */

/** user.get_profile. */
export async function fetchUserGetProfile() {
  return apiClient.user.getProfile();
}

/**
 * user.listWatched (settings notifications matrix, DEBT-06).
 * Pages through all watched repos — capped at 500; `truncated` flags the cut.
 */
export async function fetchWatchedRepos() {
  const repos: RepoPublic[] = [];
  for (let offset = 0; offset < 500; offset += 50) {
    const res = await apiClient.user.listWatched({ offset, limit: 50 });
    if (!res.ok) {
      return res;
    }
    repos.push(...res.data.repos);
    if (res.data.repos.length < 50) {
      return { ok: true as const, data: { repos, truncated: false } };
    }
  }
  return { ok: true as const, data: { repos, truncated: true } };
}

/** pat.list. */
export async function fetchPatList() {
  return apiClient.pat.list();
}

/** sshKey.list. */
export async function fetchSshKeyList() {
  return apiClient.sshKey.list();
}

/** oauthApp.list (developer apps). */
export async function fetchOAuthAppList() {
  return apiClient.oauthApp.list();
}

/** oauthApp.listGrants (authorized apps). */
export async function fetchOAuthGrantList() {
  return apiClient.oauthApp.listGrants();
}

/** oauthApp.authorizeInfo (consent screen payload). */
export async function fetchOAuthAuthorizeInfo(data: {
  client_id?: string;
  redirect_uri?: string;
  scope?: string;
}) {
  return apiClient.oauthApp.authorizeInfo({
    client_id: data.client_id ?? "",
    redirect_uri: data.redirect_uri,
    scope: data.scope,
  });
}

/** gpgKey.list. */
export async function fetchGpgKeyList() {
  return apiClient.gpgKey.list();
}

/** email.list. */
export async function fetchEmailList() {
  return apiClient.email.list();
}

/** admin.auth.getSettings. */
export async function fetchAdminAuthSettings() {
  return apiClient.admin.auth.getSettings();
}

/** admin.lfs.getSettings. */
export async function fetchAdminLfsSettings() {
  return apiClient.admin.lfs.getSettings();
}

/** admin.lfs.getUsage. */
export async function fetchAdminLfsUsage() {
  return apiClient.admin.lfs.getUsage();
}

/** admin.mcp.getSettings. */
export async function fetchAdminMcpSettings() {
  return apiClient.admin.mcp.getSettings();
}

/** admin.users.list. */
export async function fetchAdminUsersList(data: {
  query?: string | null;
  limit?: number | null;
  offset?: number | null;
}) {
  return apiClient.admin.users.list({
    query: data.query ?? null,
    limit: typeof data.limit === "number" ? data.limit : 50,
    offset: typeof data.offset === "number" ? data.offset : 0,
  });
}

/** admin.invites.list. */
export async function fetchAdminInvitesList() {
  return apiClient.admin.invites.list();
}

/** system.health (status page). */
export async function fetchSystemHealth() {
  return apiClient.system.health();
}

function documentCookie(): string {
  return typeof document !== "undefined" ? document.cookie : "";
}

/**
 * Resolved Shiki theme from theme cookies — mirrors the cookie/resolved-scheme
 * chain the serving middleware applies to `<html>`.
 */
export function resolveClientHighlightTheme(): "oxidean-light" | "oxidean-dark" {
  const cookie = documentCookie();
  const resolved = resolveThemeForSsr(
    themePreferenceFromCookieHeader(cookie),
    null,
    resolvedColorSchemeFromCookieHeader(cookie),
  );
  return resolved === "dark" ? "oxidean-dark" : "oxidean-light";
}

export type AppAccessRedirectInput = {
  pathname: string;
  needsSetup: boolean;
  mustChangeCredentials: boolean;
};

/**
 * Pure app-access gate (D-09/D-10 + UI-SPEC must_change).
 * needs_setup wins over must_change. Returns redirect path or null (no redirect).
 * Carve-outs: /status (always); /setup while needs_setup; /setup/credentials while must_change.
 */
export function resolveAppAccessRedirect(input: AppAccessRedirectInput): string | null {
  const pathname = normalizePathname(input.pathname);

  if (input.needsSetup) {
    if (pathname === "/status" || pathname === "/setup") {
      return null;
    }
    return "/setup";
  }

  if (input.mustChangeCredentials) {
    if (pathname === "/status" || pathname === "/setup/credentials") {
      return null;
    }
    return "/setup/credentials";
  }

  return null;
}

function normalizePathname(pathname: string): string {
  if (!pathname) return "/";
  const noQuery = pathname.split("?")[0] ?? "/";
  if (noQuery.length > 1 && noQuery.endsWith("/")) {
    return noQuery.slice(0, -1);
  }
  return noQuery || "/";
}
