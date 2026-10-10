import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";
import octane from "@octanejs/astro";
import tailwindcss from "@tailwindcss/vite";
import {
  fixTypeOnlyImports,
  RECHARTS_TYPE_ONLY_IMPORT_FIX,
} from "./vite-plugins/fix-type-only-imports.ts";
import { swBuildIdPlugin } from "./vite-plugins/sw-build-id.ts";
import { parseViteAllowedHosts } from "./vite-plugins/vite-allowed-hosts.ts";
import { webmcpWellKnownPlugin } from "./vite-plugins/webmcp-well-known.ts";

const rootDir = path.dirname(fileURLToPath(import.meta.url));

// Dev/preview upstream — same env contract as `oxidean-web` so `make dev`
// behaves like the composed stack: cookies and RPC stay same-origin.
const apiProxyTarget =
  process.env.OXIDEAN_API_ORIGIN?.replace(/\/$/, "") ||
  process.env.OXIDEAN_E2E_API_ORIGIN?.replace(/\/$/, "") ||
  "http://127.0.0.1:8080";

/**
 * Hostnames the dev/preview servers may answer for (DNS-rebinding guard).
 * On Railway the edge already sets Host; allow all hosts there so gateway /
 * custom domains are not blocked when env parsing fails at preview boot.
 */
function resolveViteAllowedHosts() {
  if (process.env.RAILWAY_ENVIRONMENT || process.env.RAILWAY_PROJECT_ID) {
    return true;
  }
  const hosts = parseViteAllowedHosts(
    process.env.OXIDEAN_VITE_ALLOWED_HOSTS,
    process.env.OXIDEAN_PUBLIC_ORIGIN,
  );
  return hosts.length > 0 ? hosts : undefined;
}

// Mirrors `API_PREFIXES` in crates/oxidean-web/src/proxy.rs — keep in sync.
// `/health` stays unproxied (compose/Dockerfile probe it on the web tier).
// Git smart-HTTP (`/{owner}/{repo}.git/*`) is pattern-based, not a prefix, so
// it cannot live here — it needs Traefik/oxidean-web (as before this PR).
const apiProxy = {
  "/api/rpc/ws": { target: apiProxyTarget.replace(/^http/, "ws"), ws: true },
  "/api/rpc": { target: apiProxyTarget, changeOrigin: true },
  // AGT-02: in-page WebMCP tools call the instance MCP endpoint same-origin.
  "/api/mcp": { target: apiProxyTarget, changeOrigin: true },
  "/api/auth": { target: apiProxyTarget, changeOrigin: true },
  "/api/user": { target: apiProxyTarget, changeOrigin: true },
  "/api/repos": { target: apiProxyTarget, changeOrigin: true },
  "/api/admin": { target: apiProxyTarget, changeOrigin: true },
  "/api/releases": { target: apiProxyTarget, changeOrigin: true },
  "/uploads": { target: apiProxyTarget, changeOrigin: true },
  // Package registry prefixes (D-PKG-01) — same-host paths → API.
  "/v2": { target: apiProxyTarget, changeOrigin: true },
  "/npm": { target: apiProxyTarget, changeOrigin: true },
  "/generic": { target: apiProxyTarget, changeOrigin: true },
  // CLI distribution (install.sh + ox binaries) is API-owned.
  "/cli": { target: apiProxyTarget, changeOrigin: true },
  // API-03 OAuth2 endpoints are API-owned; `/oauth/consent` stays a SPA
  // route so the consent screen renders in the web app.
  "/oauth/authorize": { target: apiProxyTarget, changeOrigin: true },
  "/oauth/token": { target: apiProxyTarget, changeOrigin: true },
  "/oauth/userinfo": { target: apiProxyTarget, changeOrigin: true },
};

/**
 * Astro builds the per-route documents; the Rust web tier serves them as
 * per-pattern shells (no JS runtime in the serving path). Octane components
 * mount as `client:load` islands — SSR HTML paints first, hydration wires
 * interactivity; view transitions come from `ClientRouter`.
 */
export default defineConfig({
  output: "static",
  integrations: [octane()],
  server: {
    port: 3000,
    allowedHosts: resolveViteAllowedHosts(),
  },
  vite: {
    plugins: [
      // Per-deploy CACHE_NAME + VITE_OXIDEAN_SW_BUILD for SW update busting.
      swBuildIdPlugin(),
      // AGT-02: /.well-known/{webmcp,mcp} discovery documents — served by
      // oxidean-web in production; middleware keeps them real in dev/preview.
      webmcpWellKnownPlugin(),
      fixTypeOnlyImports(RECHARTS_TYPE_ONLY_IMPORT_FIX),
      tailwindcss(),
    ],
    server: {
      proxy: apiProxy,
    },
    preview: {
      allowedHosts: resolveViteAllowedHosts(),
      proxy: apiProxy,
    },
    resolve: {
      alias: {
        "@": path.resolve(rootDir, "./src"),
        // Published attr-accept "module" build is fake ESM (`exports` in browser →
        // hydration abort). Bun also nests it so optimizeDeps.include cannot resolve.
        "attr-accept": path.resolve(rootDir, "./src/shims/attr-accept.ts"),
      },
    },
  },
});
