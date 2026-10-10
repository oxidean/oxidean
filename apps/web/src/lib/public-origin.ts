/**
 * Browser-facing site origin for clone URLs and other absolute links.
 * Prefer configured public origin (same knob as API magic links).
 * On Railway PR Environments, replace a stale `*.up.railway.app` configured
 * origin with the live gateway host. Custom domains are left alone.
 */

function originHostname(origin: string): string | null {
  const trimmed = origin.trim().replace(/\/$/, "");
  if (!trimmed) return null;
  try {
    const u = new URL(trimmed.includes("://") ? trimmed : `https://${trimmed}`);
    return u.hostname || null;
  } catch {
    return null;
  }
}

function isRailwayAppHost(host: string): boolean {
  const h = host.toLowerCase();
  return h === "up.railway.app" || h.endsWith(".up.railway.app");
}

/** Railway gateway hostname when the web process runs on Railway. */
function railwayGatewayHost(env: NodeJS.ProcessEnv = process.env): string | null {
  for (const key of ["RAILWAY_SERVICE_GATEWAY_URL", "RAILWAY_PUBLIC_DOMAIN"] as const) {
    const raw = env[key]?.trim();
    if (!raw) continue;
    const host = originHostname(raw);
    if (host) return host;
  }
  return null;
}

function railwayGatewayOrigin(env: NodeJS.ProcessEnv = process.env): string | null {
  const host = railwayGatewayHost(env);
  return host ? `https://${host}` : null;
}

export function resolvePublicOriginFromEnv(env: NodeJS.ProcessEnv = process.env): string | null {
  const configured = (
    env.OXIDEAN_PUBLIC_ORIGIN?.trim() ||
    env.OXIDEAN_COMPOSE_PUBLIC_ORIGIN?.trim() ||
    ""
  ).replace(/\/$/, "");
  const railway = railwayGatewayOrigin(env);

  if (configured && railway) {
    const cfgHost = originHostname(configured);
    const rwHost = originHostname(railway);
    if (
      cfgHost &&
      rwHost &&
      isRailwayAppHost(cfgHost) &&
      cfgHost.toLowerCase() !== rwHost.toLowerCase()
    ) {
      return railway;
    }
    return configured;
  }
  if (railway) return railway;
  if (configured) return configured;
  return null;
}

/** Client-safe fallback when store/loader origin is missing (tests / edge). */
export function resolvePublicOriginClient(): string {
  if (typeof window !== "undefined" && window.location?.origin) {
    return window.location.origin.replace(/\/$/, "");
  }
  return resolvePublicOriginFromEnv() ?? "http://localhost";
}

/** HTTPS clone remote for a repo (absolute — git clients need a full URL). */
export function httpsCloneUrl(origin: string, owner: string, repo: string): string {
  const base = (origin || resolvePublicOriginClient()).replace(/\/$/, "");
  return `${base}/${owner}/${repo}.git`;
}

/**
 * scp-style SSH clone URL (D-SSH-02).
 * Always `git@{host}:{owner}/{repo}.git` — port is never embedded; use
 * `sshNeedsPortHint` / `~/.ssh/config Port` when advertised port ≠ 22.
 */
export function sshCloneUrl(host: string, _port: number, owner: string, repo: string): string {
  const h = host
    .trim()
    .replace(/\/$/, "")
    .replace(/^\[|\]$/g, "");
  return `git@${h}:${owner}/${repo}.git`;
}

/** True when clients need an explicit SSH Port (not the default 22). */
export function sshNeedsPortHint(port: number): boolean {
  return Number.isFinite(port) && port > 0 && port !== 22;
}

/**
 * Meta tag content injected by the serving middleware (the tier that can read
 * OXIDEAN_SSH_* env). Empty string when absent.
 */
function advertiseMeta(name: string): string {
  if (typeof document === "undefined") return "";
  return document.querySelector(`meta[name="${name}"]`)?.getAttribute("content")?.trim() ?? "";
}

/**
 * Advertised SSH hostname from `<meta name="oxidean:ssh-host">` (middleware
 * injected); falls back to the public origin's hostname.
 */
export function resolveSshAdvertiseHost(publicOrigin?: string): string {
  const fromMeta = advertiseMeta("oxidean:ssh-host");
  if (fromMeta) {
    return fromMeta;
  }
  const origin = (publicOrigin || "").trim() || resolvePublicOriginClient();
  try {
    const u = new URL(origin.includes("://") ? origin : `http://${origin}`);
    return u.hostname || "localhost";
  } catch {
    return "localhost";
  }
}

/** Advertised SSH port from `<meta name="oxidean:ssh-port">`; default 2222. */
export function resolveSshAdvertisePort(): number {
  const n = Number.parseInt(advertiseMeta("oxidean:ssh-port"), 10);
  if (Number.isFinite(n) && n > 0) return n;
  return 2222;
}
