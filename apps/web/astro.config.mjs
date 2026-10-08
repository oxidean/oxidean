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

const rootDir = path.dirname(fileURLToPath(import.meta.url));

/**
 * Astro builds the per-route documents; the Rust web tier serves them as
 * per-pattern shells (no JS runtime in the serving path). Octane components
 * mount as `client:load` islands — SSR HTML paints first, hydration wires
 * interactivity; view transitions come from `ClientRouter`.
 */
export default defineConfig({
  output: "static",
  integrations: [octane()],
  vite: {
    plugins: [
      // Per-deploy CACHE_NAME + VITE_OXIDEAN_SW_BUILD for SW update busting.
      swBuildIdPlugin(),
      fixTypeOnlyImports(RECHARTS_TYPE_ONLY_IMPORT_FIX),
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
  },
});
