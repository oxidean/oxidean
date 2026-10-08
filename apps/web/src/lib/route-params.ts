/**
 * URL pattern matching for islands — the post-router replacement for
 * `useParams` / splat params.
 *
 * Patterns look like `/$owner/$repo/issues/$n` where a `$name` segment binds
 * one path segment and a bare `$` binds the rest (splat). Segments are
 * percent-decoded.
 */
export function matchPath(pattern: string, pathname: string): Record<string, string> | null {
  const patternSegs = pattern.split("/").filter(Boolean);
  const pathSegs = pathname.split("/").filter(Boolean);
  const params: Record<string, string> = {};

  let i = 0;
  for (; i < patternSegs.length; i++) {
    const pseg = patternSegs[i]!;
    if (pseg === "$") {
      params._splat = decodeURIComponent(pathSegs.slice(i).join("/"));
      return params;
    }
    if (i >= pathSegs.length) return null;
    const seg = decodeURIComponent(pathSegs[i]!);
    if (pseg.startsWith("$")) params[pseg.slice(1)] = seg;
    else if (pseg !== seg) return null;
  }
  return i === pathSegs.length ? params : null;
}

/** Current URL search params — the post-router `useSearch` replacement. */
export function readSearchParams(): URLSearchParams {
  return new URLSearchParams(window.location.search);
}

/** Search string → `Record` for former `validateSearch` functions. */
export function searchRecord(searchStr: string): Record<string, unknown> {
  return Object.fromEntries(new URLSearchParams(searchStr.replace(/^\?/, "")));
}

/**
 * First path segments that never bind `{owner}`/`{repo}` — every top-level
 * app route that precedes the `/{owner}…` pattern in specificity.
 */
const RESERVED_TOP_SEGMENTS = new Set([
  "admin",
  "api",
  "dashboard",
  "explore",
  "invites",
  "login",
  "new",
  "notifications",
  "oauth",
  "orgs",
  "reset-password",
  "search",
  "settings",
  "setup",
  "signup",
  "status",
  "verify",
]);

/** Second segments under `/{owner}/…` that are literal routes, not repo names. */
const RESERVED_OWNER_SEGMENTS = new Set(["packages", "settings"]);

/**
 * `{owner, repo}` when the pathname matches `/{owner}/{repo}/…` — null for
 * reserved/app routes (`/settings/general` is not owner/repo). The post-router
 * replacement for `useParams({ strict: false })` repo-context reads.
 */

export function repoParamsFromPathname(
  pathname = typeof window !== "undefined" ? window.location.pathname : "/",
): { owner: string; repo: string } | null {
  const m = matchPath("/$owner/$repo/$", pathname) ?? matchPath("/$owner/$repo", pathname);
  if (!m || RESERVED_TOP_SEGMENTS.has(m.owner ?? "") || RESERVED_OWNER_SEGMENTS.has(m.repo ?? "")) {
    return null;
  }
  return { owner: m.owner!, repo: m.repo! };
}

/**
 * Build an href from a route pattern + params — the post-router replacement
 * for `navigate({ to, params, search })` and `Link` `to`/`params`.
 *
 * `search` accepts a record or a URLSearchParams-ready value; empty/falsey
 * values are dropped. Splat (`$`) binds `_splat`.
 */
export function routeHref(
  pattern: string,
  params: Record<string, string | number | undefined | null> = {},
  search?: Record<string, unknown> | URLSearchParams | null,
): string {
  const segs = pattern.split("/").filter(Boolean);
  const out: string[] = [];
  for (const seg of segs) {
    if (seg === "$") {
      const splat = params._splat;
      if (splat) out.push(...String(splat).split("/").map(encodeURIComponent));
      continue;
    }
    if (seg.startsWith("$")) {
      const v = params[seg.slice(1)];
      if (v == null) return "#";
      out.push(encodeURIComponent(String(v)));
      continue;
    }
    out.push(seg);
  }
  let href = "/" + out.join("/");
  if (search) {
    const qs = search instanceof URLSearchParams ? search : new URLSearchParams();
    if (!(search instanceof URLSearchParams)) {
      for (const [k, v] of Object.entries(search)) {
        if (v == null || v === "") continue;
        qs.set(
          k,
          typeof v === "string" || typeof v === "number" || typeof v === "boolean"
            ? String(v)
            : JSON.stringify(v),
        );
      }
    }
    const q = qs.toString();
    if (q) href += `?${q}`;
  }
  return href;
}
