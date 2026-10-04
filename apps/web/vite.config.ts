import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { tanstackStart } from "@octanejs/tanstack-start/plugin/vite";
import tailwindcss from "@tailwindcss/vite";
import {
  fixTypeOnlyImports,
  RECHARTS_TYPE_ONLY_IMPORT_FIX,
} from "./vite-plugins/fix-type-only-imports.ts";
import { parseViteAllowedHosts } from "./vite-plugins/vite-allowed-hosts.ts";
import { webHealthPlugin } from "./vite-plugins/web-health.ts";
import { webmcpWellKnownPlugin } from "./vite-plugins/webmcp-well-known.ts";
import { swBuildIdPlugin } from "./vite-plugins/sw-build-id.ts";
import { appStylesHrefPlugin } from "./vite-plugins/app-styles-href.ts";

// NOTE (03-05): vite-plugin-pwa was evaluated here but does not emit a service
// worker under this Vite 8 / @octanejs/tanstack-start multi-environment build
// (the manifest virtual module resolves, but the workbox-build closeBundle
// hook never fires — no sw.js is produced, silently). Per the 03-05 plan's
// documented fallback, the manifest and service worker are hand-authored as
// static files in apps/web/public/ instead (manifest.webmanifest, sw.js).
// swBuildIdPlugin stamps CACHE_NAME + VITE_OXIDEAN_SW_BUILD per deploy so
// clients install a fresh worker instead of pinning an old shell cache.
const rootDir = path.dirname(fileURLToPath(import.meta.url));

/**
 * Hostnames Vite may serve (DNS-rebinding guard).
 * Resolved inside defineConfig so Railway/runtime env is visible (not build-time).
 * On Railway the edge already sets Host; allow all hosts there so gateway /
 * custom domains are not blocked when env parsing fails at preview boot.
 */
function resolveViteAllowedHosts(): true | string[] | undefined {
  if (process.env.RAILWAY_ENVIRONMENT || process.env.RAILWAY_PROJECT_ID) {
    return true;
  }
  const hosts = parseViteAllowedHosts(
    process.env.OXIDEAN_VITE_ALLOWED_HOSTS,
    process.env.OXIDEAN_PUBLIC_ORIGIN,
  );
  return hosts.length > 0 ? hosts : undefined;
}

export default defineConfig(() => {
  const apiProxyTarget =
    process.env.OXIDEAN_API_ORIGIN?.replace(/\/$/, "") ||
    process.env.OXIDEAN_E2E_API_ORIGIN?.replace(/\/$/, "") ||
    "http://127.0.0.1:8080";
  const viteAllowedHosts = resolveViteAllowedHosts();

  return {
    plugins: [
      // Before Start/proxy: Compose + Dockerfile probe `/health` with Oxidean-Health-Probe.
      webHealthPlugin(),
      // Per-deploy CACHE_NAME + VITE_OXIDEAN_SW_BUILD for SW update busting.
      swBuildIdPlugin(),
      // SSR-side stylesheet href must be the file the client build emitted.
      appStylesHrefPlugin(),
      // AGT-02: /.well-known/{webmcp,mcp} discovery documents (dev + preview).
      webmcpWellKnownPlugin(),
      fixTypeOnlyImports(RECHARTS_TYPE_ONLY_IMPORT_FIX),
      tanstackStart({
        // Keep colocated *.integration.test.* / *.unit.test.* out of the route tree
        // (avoids noisy warnings and extra SSR work during stack e2e).
        // Must live under `router` — top-level keys are stripped by Start's schema.
        router: {
          routeFileIgnorePattern: "\\.(test|spec)\\.",
        },
      }),
      tailwindcss(),
    ],
    resolve: {
      alias: {
        "@": path.resolve(rootDir, "./src"),
        // Published attr-accept "module" build is fake ESM (`exports` in browser →
        // hydration abort). Bun also nests it so optimizeDeps.include cannot resolve.
        "attr-accept": path.resolve(rootDir, "./src/shims/attr-accept.ts"),
      },
    },
    server: {
      port: 3000,
      allowedHosts: viteAllowedHosts,
      proxy: {
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
        // `/health` is owned by webHealthPlugin (header-gated). Public API /health
        // remains via Traefik → api in Compose; do not proxy here.
        // Phase 20 package registry (D-PKG-01) — same-host path prefixes → API
        "/v2": { target: apiProxyTarget, changeOrigin: true },
        "/npm": { target: apiProxyTarget, changeOrigin: true },
        "/generic": { target: apiProxyTarget, changeOrigin: true },
        // API-03 OAuth2 endpoints are API-owned; `/oauth/consent` stays a SPA
        // route so the consent screen renders in the web app.
        "/oauth/authorize": { target: apiProxyTarget, changeOrigin: true },
        "/oauth/token": { target: apiProxyTarget, changeOrigin: true },
        "/oauth/userinfo": { target: apiProxyTarget, changeOrigin: true },
      },
    },
    preview: {
      allowedHosts: viteAllowedHosts,
    },
  };
});
