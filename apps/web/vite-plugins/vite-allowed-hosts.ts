/**
 * Parse Vite `server` / `preview` allowedHosts from env (cloud / Compose).
 *
 * Domains stay out of source — operators pass hosts at runtime.
 * Leading-dot entries (e.g. `.up.railway.app`) allow that host and all
 * subdomains (Vite semantics).
 */
export function parseViteAllowedHosts(
  rawHosts: string | undefined,
  publicOrigin: string | undefined,
): string[] {
  const hosts = new Set<string>();

  for (const part of (rawHosts ?? "").split(",")) {
    const host = part.trim().toLowerCase();
    if (host) hosts.add(host);
  }

  const origin = (publicOrigin ?? "").trim();
  if (origin) {
    try {
      const { hostname } = new URL(origin);
      if (hostname) hosts.add(hostname.toLowerCase());
    } catch {
      // Ignore malformed OXIDEAN_PUBLIC_ORIGIN; CORS/API config owns validation.
    }
  }

  return [...hosts];
}
