/**
 * `@octanejs/astro` skips the Octane transform for every `node_modules` path
 * outside `@octanejs/*` and `octane` (should-skip-transform.js). Raw-source
 * Octane packages under other scopes — `@tanstack/ai-octane` ships `.tsrx`
 * hook modules — then reach rolldown unparsed, where `import type` fails
 * with `Expected 'from' but found '{'`.
 *
 * This plugin resolves `.tsrx`/`.tsx` modules inside `@tanstack/ai-octane`
 * to virtual ids with no `node_modules` segment, so the compiler owns them
 * by extension exactly like project `.tsrx`. Relative and bare imports from
 * virtualized modules resolve back into the real package directory.
 */
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import type { Plugin } from "vite";

const PKG = "@tanstack/ai-octane";
/** Virtual id prefix — deliberately `\0`-free and `node_modules`-free. */
const VIRTUAL = "tanstack-ai-octane-octane-src:";
const EXTENSIONS = [".tsrx", ".tsx", ".ts", ".mts", ".mjs", ".jsx", ".js", ".json"];

function resolveFile(base: string): string | null {
  if (fs.existsSync(base) && fs.statSync(base).isFile()) return base;
  for (const ext of EXTENSIONS) {
    const candidate = `${base}${ext}`;
    if (fs.existsSync(candidate)) return candidate;
  }
  if (fs.existsSync(base) && fs.statSync(base).isDirectory()) {
    for (const ext of EXTENSIONS) {
      const candidate = path.join(base, `index${ext}`);
      if (fs.existsSync(candidate)) return candidate;
    }
  }
  return null;
}

export function tanstackAiOctaneSource(projectRoot: string): Plugin {
  const require = createRequire(path.join(projectRoot, "package.json"));

  let pkgSrcDir: string | null = null;
  function pkgSrc(): string {
    if (pkgSrcDir === null) {
      // `exports` resolves the entry directly; the manifest itself is not exported.
      const entry = require.resolve(PKG);
      pkgSrcDir = path.dirname(entry);
    }
    return pkgSrcDir;
  }

  function virtualId(real: string): string {
    return VIRTUAL + path.relative(pkgSrc(), real).split(path.sep).join("/");
  }

  function realPath(id: string): string {
    return path.join(pkgSrc(), ...id.slice(VIRTUAL.length).split("/"));
  }

  function insidePkg(file: string): boolean {
    const rel = path.relative(pkgSrc(), file);
    return rel !== "" && !rel.startsWith("..") && !path.isAbsolute(rel);
  }

  return {
    name: "tanstack-ai-octane-source",
    enforce: "pre",

    resolveId(source, importer) {
      const spec = source.split("?")[0] ?? source;

      // Relative imports from a virtualized module resolve inside the real
      // package source tree; only `.tsrx`/`.tsx` results are virtualized.
      if (importer?.startsWith(VIRTUAL)) {
        const importerReal = realPath(importer);
        if (spec.startsWith(".")) {
          const real = resolveFile(path.resolve(path.dirname(importerReal), spec));
          if (real === null) return null;
          return /\.(tsrx|tsx)$/.test(real) ? virtualId(real) : real;
        }
        // Bare imports (e.g. `@tanstack/ai-client/devtools`) must resolve
        // from the real package directory — the virtual id has no dirname
        // for node-resolution to walk. `this.resolve` can throw/return
        // null under rolldown; node's own resolver honors the exports map.
        return this.resolve(spec, importerReal, { skipSelf: true })
          .then((resolved) => resolved?.id ?? null)
          .catch(() => null)
          .then((id) => {
            // A resolved id that still equals the bare specifier was
            // externalized, not resolved — fall through to ESM
            // resolution, which honors `import`-condition exports maps
            // (`require.resolve` cannot read them).
            if (id && id !== spec) return id;
            try {
              return fileURLToPath(import.meta.resolve(spec, pathToFileURL(importerReal).href));
            } catch {
              return null;
            }
          });
      }

      // Relative imports inside the real package (reached via its `.ts`
      // entry): virtualize only `.tsrx`/`.tsx` targets.
      if (spec.startsWith(".") && importer !== undefined && insidePkg(importer)) {
        const real = resolveFile(path.resolve(path.dirname(importer), spec));
        return real !== null && /\.(tsrx|tsx)$/.test(real) ? virtualId(real) : null;
      }
      return null;
    },

    load(id) {
      if (!id.startsWith(VIRTUAL)) return null;
      // `load` returns the authored source; the Octane transform compiles it.
      // The real file is not a graph node, so dep edits don't invalidate the
      // dev graph — restart the dev server when patching the package.
      return fs.readFileSync(realPath(id), "utf8");
    },
  };
}
