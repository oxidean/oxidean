/**
 * Resolve the deployed stylesheet href for the SSR document.
 *
 * `styles.css?url` resolves independently inside the client and SSR bundles
 * and the two pipelines hash different bytes — the SSR <link> can then point
 * at a filename the client build never emitted, so deployed pages fetch the
 * SPA fallback instead of CSS (#104). The SSR environment therefore reads
 * the exact filename the client environment already emitted (Start builds
 * client before ssr); the client bundle and dev server keep resolving `?url`
 * normally, which is already correct there.
 */
import { existsSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import type { Plugin } from "vite";

export const APP_STYLES_HREF_ID = "virtual:oxidean-app-styles-href";
const RESOLVED_ID = "\0" + APP_STYLES_HREF_ID;
const STYLES_ASSET = /^styles(?:-.+)?\.css$/;

export function appStylesHrefPlugin(): Plugin {
  let clientAssetsDir = "";

  return {
    name: "oxidean-app-styles-href",
    configResolved(config) {
      // dist/client is fixed by the Start build layout (same assumption as
      // swBuildIdPlugin).
      clientAssetsDir = resolve(config.root, "dist", "client", "assets");
    },
    resolveId(id) {
      return id === APP_STYLES_HREF_ID ? RESOLVED_ID : undefined;
    },
    load(id) {
      if (id !== RESOLVED_ID) return;
      // Dev serves the transformed stylesheet at its module path, and in the
      // client bundle `?url` already resolves to the file that bundle emits.
      if (this.environment.mode === "dev" || this.environment.name === "client") {
        return `export { default } from "@/styles.css?url";`;
      }
      if (!existsSync(clientAssetsDir)) {
        this.error(
          `app-styles-href: ${clientAssetsDir} is missing — the client ` +
            `environment must build before SSR`,
        );
      }
      const files = readdirSync(clientAssetsDir)
        .filter((f) => STYLES_ASSET.test(f))
        .sort();
      if (files.length !== 1) {
        this.error(
          `app-styles-href: expected exactly one styles*.css in ` +
            `${clientAssetsDir}, found ${files.length}`,
        );
      }
      return `export default ${JSON.stringify(`/assets/${files[0]}`)};`;
    },
  };
}
