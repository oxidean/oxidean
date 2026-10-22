#!/usr/bin/env bun
/**
 * A stray `;` in JSX children position is a literal semicolon text node in the
 * rendered page — `</X>;` between siblings (or as a parent's last child) ships
 * visible punctuation ("Get started ; Explore" on the landing page, stray `;`
 * between repo settings panels). `;` after the root node of an `@{ }` template
 * or inside `@if`/`@for` arms is the statement terminator and never renders —
 * only JSXText children count.
 *
 * Line-level greps cannot tell the two apart (the shipped bug was
 * `</AppLink>;` end-of-line, which oxfmt preserves or merges into `>;<`), so
 * this check parses each file with @tsrx/oxc and flags `JSXText` nodes whose
 * content is nothing but `;`. To render a literal semicolon on purpose, write
 * it as an expression (`{";"}`) — that is the opt-out.
 */
import { parseSync } from "@tsrx/oxc/parser";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..", "apps", "web", "src");

type Hit = { file: string; line: number };

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (/\.tsx?$/.test(name) || name.endsWith(".tsrx")) out.push(p);
  }
  return out;
}

const STRAY_SEMICOLON = /^\s*;+\s*$/;

function lineOf(src: string, offset: number): number {
  let line = 1;
  for (let i = 0; i < offset && i < src.length; i++) {
    if (src.charCodeAt(i) === 10) line++;
  }
  return line;
}

function visit(node: unknown, src: string, file: string, hits: Hit[]): void {
  if (!node || typeof node !== "object") return;
  if (Array.isArray(node)) {
    for (const child of node) visit(child, src, file, hits);
    return;
  }
  const n = node as Record<string, unknown>;
  if (n.type === "JSXText" && typeof n.value === "string" && STRAY_SEMICOLON.test(n.value)) {
    const start = typeof n.start === "number" ? n.start : 0;
    hits.push({ file, line: lineOf(src, start) });
    return;
  }
  for (const [key, value] of Object.entries(n)) {
    if (key === "start" || key === "end" || key === "loc" || key === "range") continue;
    if (value && typeof value === "object") visit(value, src, file, hits);
  }
}

const hits: Hit[] = [];
const unparsed: string[] = [];
for (const file of walk(ROOT)) {
  const rel = relative(ROOT, file);
  const src = readFileSync(file, "utf-8");
  const lang = file.endsWith(".tsrx") ? "tsrx" : file.endsWith(".tsx") ? "tsx" : "ts";
  const res = parseSync(file, src, { lang, sourceType: "module" });
  // Oxlint (same parser, earlier in `make web-lint`) is the syntax gate — a
  // file that fails to parse is reported there; here we just cannot scan it.
  if (!res.program) {
    unparsed.push(rel);
    continue;
  }
  visit(res.program, src, rel, hits);
}

if (hits.length === 0 && unparsed.length === 0) {
  console.log("check-template-semicolons: ok");
  process.exit(0);
}

if (hits.length > 0) {
  console.error("check-template-semicolons: stray `;` in JSX children:\n");
  for (const h of hits) {
    console.error(`  ${h.file}:${h.line}`);
  }
  console.error(
    '\nA `;` inside JSX children renders as a literal semicolon on the page. Remove it — ' +
      'statement terminators only belong after a template\'s root node or inside @if/@for ' +
      'arms. To render a literal semicolon deliberately, write it as an expression: `{";"}`.',
  );
}
if (unparsed.length > 0) {
  console.error("\ncheck-template-semicolons: files that failed to parse (see oxlint output):\n");
  for (const f of unparsed) console.error(`  ${f}`);
}
process.exit(1);
