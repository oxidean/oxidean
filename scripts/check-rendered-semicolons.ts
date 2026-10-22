#!/usr/bin/env bun
/**
 * Post-build rendered-output gate for issue #165-class leaks.
 *
 * `astro build` prerenders every page — this scans `apps/web/dist/**​/*.html`
 * for `;`-only text nodes inside layout elements: the exact shape that
 * shipped "Get started ; Explore" on the landing page. Source-side coverage
 * is scripts/check-template-semicolons.ts (parse-level, per file); this is
 * the all-pages verification at the layer the bug actually appeared in.
 *
 * Requires apps/web/dist — run `bun run --filter @oxidean/web build` first.
 * Wired into CI after the web build (web-octane job) and `make web-render-check`.
 */
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { findStrayTemplateText } from "../apps/web/src/test/stray-template-text.ts";

const ROOT = join(import.meta.dir, "..");
const DIST = join(ROOT, "apps", "web", "dist");

type Hit = { file: string; line: number; parent: string };

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (name.endsWith(".html")) out.push(p);
  }
  return out;
}

function lineOf(src: string, offset: number): number {
  let line = 1;
  for (let i = 0; i < offset && i < src.length; i++) {
    if (src.charCodeAt(i) === 10) line++;
  }
  return line;
}

if (!existsSync(DIST)) {
  console.error(
    "check-rendered-semicolons: apps/web/dist is missing — " +
      "run `bun run --filter @oxidean/web build` first.",
  );
  process.exit(1);
}

const hits: Hit[] = [];
for (const file of walk(DIST)) {
  const rel = relative(ROOT, file);
  const html = readFileSync(file, "utf-8");
  for (const stray of findStrayTemplateText(html)) {
    hits.push({ file: rel, line: lineOf(html, stray.offset), parent: stray.parent });
  }
}

if (hits.length === 0) {
  console.log("check-rendered-semicolons: ok");
  process.exit(0);
}

console.error("check-rendered-semicolons: stray `;` text in rendered pages:\n");
for (const h of hits) {
  console.error(`  ${h.file}:${h.line} (inside <${h.parent}>)`);
}
console.error(
  "\nA `;` inside JSX children renders as a literal semicolon on the page — " +
    "the same leak scripts/check-template-semicolons.ts gates at source level. " +
    'A deliberate visible semicolon belongs in a text element: <span>{";"}</span>.',
);
process.exit(1);
