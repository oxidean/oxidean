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

/** Deepest string in an `exports` condition tree — `import` before `default`. */
function pickCondition(v: unknown): string | null {
  if (typeof v === "string") return v;
  if (v && typeof v === "object") {
    const o = v as Record<string, unknown>;
    return pickCondition(o.import) ?? pickCondition(o.default);
  }
  return null;
}

/** The `exports[sub]` target for `sub` like `"."`/`"./x"`, or null. */
function pickExport(exports: unknown, key: string): string | null {
  if (exports === null || typeof exports !== "object") return null;
  const table = exports as Record<string, unknown>;
  // `{".": {...}}` and bare condition objects (`{"import": "..."}`) both
  // appear as the package root's exports value.
  const entry = key in table ? table[key] : key === "." ? table : null;
  return pickCondition(entry);
}

/**
 * Resolve a bare specifier as if imported from `fromFile` — node_modules
 * walk upward from the importer's directory, `exports`-map aware
 * (`import`/`default` conditions). `import.meta.resolve` ignores its
 * `parent` arg without an experimental flag on pre-24 Node, and CJS
 * `require.resolve` cannot read `import` conditions.
 */
function resolveFrom(fromFile: string, spec: string): string | null {
  const seg = spec.startsWith("@")
    ? spec.split("/").slice(0, 2).join("/")
    : (spec.split("/")[0] ?? spec);
  const sub = spec.slice(seg.length);
  let dir = path.dirname(fromFile);
  for (;;) {
    const pkgDir = path.join(dir, "node_modules", seg);
    const manifestPath = path.join(pkgDir, "package.json");
    if (fs.existsSync(manifestPath)) {
      const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8")) as {
        exports?: unknown;
      };
      const target = pickExport(manifest.exports, `.${sub}`) ?? `.${sub}`;
      const hit = resolveFile(path.join(pkgDir, target));
      return hit;
    }
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
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
            // `import.meta.resolve`'s parent arg is silently ignored without
            // `--experimental-import-meta-resolve` (pre-Node-24), which would
            // resolve from this plugin module instead of `importerReal`.
            // Walk importerReal's own node_modules chain instead — CJS
            // `require.resolve` can't read `import`-condition exports maps.
            return resolveFrom(importerReal, spec);
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
      // Containment: `path.join` resolves `..` segments — an id like
      // `tanstack-ai-octane-octane-src:../../etc/passwd` must not escape
      // the package directory.
      const real = path.resolve(realPath(id));
      const root = path.resolve(pkgSrc());
      if (real !== root && !real.startsWith(root + path.sep)) {
        return null;
      }
      return fs.readFileSync(real, "utf8");
    },
  };
}
