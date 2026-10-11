/**
 * Stamp public/sw.js with a per-deploy build id and expose it to the app
 * as import.meta.env.VITE_OXIDEAN_SW_BUILD for registration cache-busting.
 */
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";
import type { Plugin } from "vite";
import { resolveSwBuildId, stampSwSource } from "../src/lib/sw-build-id.ts";

function stampSwFile(filePath: string, buildId: string): boolean {
  if (!existsSync(filePath)) return false;
  const raw = readFileSync(filePath, "utf8");
  const next = stampSwSource(raw, buildId);
  if (next === raw) return false;
  writeFileSync(filePath, next);
  return true;
}

export function swBuildIdPlugin(): Plugin {
  const buildId = resolveSwBuildId();
  let root = process.cwd();
  let clientOutDir = "";

  const stampEmitted = () => {
    const candidates = [
      clientOutDir ? join(clientOutDir, "sw.js") : "",
      join(root, "dist", "client", "sw.js"),
      join(root, "dist", "sw.js"),
    ].filter(Boolean);
    for (const filePath of candidates) {
      if (stampSwFile(filePath, buildId)) return filePath;
    }
    return null;
  };

  return {
    name: "oxidean-sw-build-id",
    // Run after Vite copies public/ into outDir so our stamp is not overwritten.
    enforce: "post",
    config() {
      return {
        define: {
          "import.meta.env.VITE_OXIDEAN_SW_BUILD": JSON.stringify(buildId),
        },
      };
    },
    configResolved(config) {
      root = config.root;
      clientOutDir = resolve(config.root, config.build.outDir);
    },
    configureServer(server) {
      // Dev: serve a stamped sw.js so CACHE_NAME is not the placeholder literal.
      server.middlewares.use((req, res, next) => {
        const url = req.url?.split("?")[0] ?? "";
        if (url !== "/sw.js") {
          next();
          return;
        }
        const filePath = join(root, "public", "sw.js");
        if (!existsSync(filePath)) {
          next();
          return;
        }
        const body = stampSwSource(readFileSync(filePath, "utf8"), buildId);
        res.setHeader("Content-Type", "application/javascript; charset=utf-8");
        res.setHeader("Cache-Control", "no-cache");
        res.end(body);
      });
    },
    writeBundle() {
      stampEmitted();
    },
    closeBundle() {
      stampEmitted();
    },
  };
}
