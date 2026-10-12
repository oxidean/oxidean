import type { KnipConfig } from "knip";

/**
 * Dead-code gate for the TypeScript side of the workspace.
 *
 * Two source kinds need custom compilers so knip can read their import
 * graphs:
 *
 *   - `.astro` pages/layouts — only the frontmatter block carries imports.
 *   - `.tsrx` Octane components — TS syntax plus `@{}`/`@if` template syntax
 *     knip's parser can't read. The import/export statements alone define the
 *     file graph, so the compiler extracts complete (possibly multi-line)
 *     `import`/`export … from`/`import(…)` statements and stubs named/default
 *     export declarations so export-usage tracking keeps working.
 *
 * What knip catches here: unused files, unused dependencies, and unused
 * exports. Symbol usage *inside* `.tsrx` templates stays with the type-aware
 * oxlint gate (`make web-lint`), which understands the template syntax.
 */
const tsrxCompiler = (text: string): string => {
  const out: string[] = [];
  // Complete import/export statements — specifier lists wrap across lines.
  for (const m of text.matchAll(
    /\b(?:import|export)\b[\s\S]*?\bfrom\s*['"][^'"]+['"]|\bimport\s*['"][^'"]+['"]|\bimport\(\s*['"][^'"]+['"]\s*\)/g,
  )) {
    out.push(m[0]);
  }
  // Named/default export declarations → same-kind stubs so knip tracks the
  // symbol (and `export type`/`import type` stays on the type channel).
  for (const m of text.matchAll(
    /\bexport\s+(declare\s+)?(default\s+)?(async\s+)?(function\*?|class|const|let|var|interface|type|enum)\s+([A-Za-z_$][\w$]*)?/g,
  )) {
    const [, , isDefault, , kind, name] = m;
    if (isDefault) {
      out.push(`export default ${name ?? "0"};`);
    } else if (kind === "interface") {
      out.push(`export interface ${name} {}`);
    } else if (kind === "type") {
      out.push(`export type ${name} = unknown;`);
    } else if (kind === "enum") {
      out.push(`export enum ${name} {}`);
    } else {
      out.push(`export const ${name} = 0;`);
    }
  }
  return out.join("\n");
};

const astroCompiler = (text: string): string =>
  text.match(/^---\n([\s\S]*?)\n---/)?.[1] ?? "";

// CSS entry: `@import "pkg"` and `@plugin "pkg"` are dependency edges
// (fontsource sheets, tailwind v4 CSS-first config).
const cssCompiler = (text: string): string =>
  [...text.matchAll(/@(?:import|plugin|use)\s+['"]([^'"]+)['"]/g)]
    .map((m) => `import ${JSON.stringify(m[1])};`)
    .join("\n");

const config: KnipConfig = {
  compilers: {
    ".tsrx": tsrxCompiler,
    ".astro": astroCompiler,
    ".css": cssCompiler,
  },
  workspaces: {
    ".": {
      entry: ["scripts/**/*.ts", "scripts/**/*.mjs"],
      project: ["scripts/**/*"],
    },
    "apps/web": {
      entry: [
        "src/pages/**/*.astro",
        "src/**/*.test.{ts,tsrx}",
        "e2e/**/*.ts",
        // Alias targets — loaded via astro.config/vitest.config resolve.alias,
        // never imported by specifier:
        "src/shims/attr-accept.ts",
        "src/test/astro-transitions-stub.ts",
        // Coverage manifests are `await import(manifestPath)`-ed by path in
        // scripts/{route,browser}-coverage-check.ts:
        "src/test/route-coverage.manifest.ts",
        "src/test/browser-coverage.manifest.ts",
      ],
      project: ["src/**/*", "vite-plugins/**/*", "e2e/**/*"],
      // `oxlint-tsgolint` is loaded by `oxlint --type-aware` (package.json
      // `lint` script), not by an import specifier.
      ignoreDependencies: ["oxlint-tsgolint"],
      // `ssh-keygen` is a system binary invoked by e2e stack helpers.
      ignoreBinaries: ["ssh-keygen"],
    },
  },
};

export default config;
