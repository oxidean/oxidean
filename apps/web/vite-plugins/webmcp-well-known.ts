/**
 * WebMCP / MCP well-known discovery documents (AGT-02).
 *
 * `/.well-known/*` paths cannot live in `public/` (dot-directories are not
 * reliably served by Vite dev/preview), so this plugin serves them from
 * middleware in both `vite dev` and `vite preview` — the same pattern as
 * `web-health.ts`. Traefik routes `/.well-known` to the web service
 * (catch-all), so these documents land on the site origin where agents look.
 *
 *   GET /.well-known/webmcp   — rel="webmcp" discovery document: how the site
 *                               surfaces browser-side tools and where the
 *                               backing MCP endpoint lives.
 *   GET /.well-known/mcp      — MCP server metadata for the instance endpoint.
 */
import type { ServerResponse } from "node:http";
import type { Connect, Plugin } from "vite";
import {
  buildMcpWellKnownDocument,
  buildWebMcpWellKnownDocument,
  MCP_WELL_KNOWN_PATH,
  WEBMCP_WELL_KNOWN_PATH,
} from "../src/lib/webmcp.ts";

function sendJson(res: ServerResponse, body: string): void {
  res.statusCode = 200;
  res.setHeader("content-type", "application/json; charset=utf-8");
  res.setHeader("cache-control", "public, max-age=300");
  res.end(body);
}

export function webmcpWellKnownMiddleware(): Connect.NextHandleFunction {
  const webmcp = JSON.stringify(buildWebMcpWellKnownDocument(), null, 2);
  const mcp = JSON.stringify(buildMcpWellKnownDocument(), null, 2);
  return (req, res, next) => {
    const path = (req.url ?? "").split("?")[0] ?? "";
    const isWebmcp = path === WEBMCP_WELL_KNOWN_PATH || path === `${WEBMCP_WELL_KNOWN_PATH}.json`;
    const isMcp = path === MCP_WELL_KNOWN_PATH || path === `${MCP_WELL_KNOWN_PATH}.json`;
    if (!isWebmcp && !isMcp) {
      next();
      return;
    }
    if (req.method !== "GET" && req.method !== "HEAD") {
      res.statusCode = 405;
      res.setHeader("allow", "GET, HEAD");
      res.end();
      return;
    }
    sendJson(res, isWebmcp ? webmcp : mcp);
  };
}

/** Vite plugin — runs in `vite dev` and `vite preview` (Compose web image). */
export function webmcpWellKnownPlugin(): Plugin {
  const handle = webmcpWellKnownMiddleware();
  return {
    name: "oxidean-webmcp-well-known",
    configureServer(server) {
      server.middlewares.use(handle);
    },
    configurePreviewServer(server) {
      server.middlewares.use(handle);
    },
  };
}
