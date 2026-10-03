#!/usr/bin/env bun
/**
 * Route loaders that read `location.searchStr` must declare `loaderDeps`.
 *
 * Router match ids are built from route id + path params + loaderDeps hash.
 * Without `loaderDeps`, every `/path?…` variant shares one match: a
 * search-only navigation "stays" the match, the loader is skipped, and
 * `useLoaderData` keeps serving the previous params' data — the page appears
 * to ignore the click until a second click on the same URL forces a reload.
 * It also lets an in-flight intent preload for one search populate the match
 * the page renders for another.
 *
 * Rule: any route file whose loader consumes `location.searchStr` must set
 * `loaderDeps` (e.g. `loaderDeps: ({ search }) => search`) so each distinct
 * search gets its own match + loaderData.
 *
 * If `location.searchStr` appears outside the route's loader (e.g. link
 * construction in the component), annotate the line/comment with
 * `loader-deps-ok` to opt out.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..", "apps", "web", "src", "routes");
const USES_SEARCH = /location\.searchStr/;
const HAS_DEPS = /\bloaderDeps\s*[:(]/;
const OPT_OUT = /loader-deps-ok/;

type Hit = { file: string; line: number; detail: string };

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (name.endsWith(".tsrx")) out.push(p);
  }
  return out;
}

function scan(file: string): Hit[] {
  const rel = relative(join(import.meta.dir, ".."), file);
  const src = readFileSync(file, "utf8");
  if (HAS_DEPS.test(src) || OPT_OUT.test(src)) return [];
  const hits: Hit[] = [];
  const lines = src.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    if (!USES_SEARCH.test(lines[i]!)) continue;
    hits.push({
      file: rel,
      line: i + 1,
      detail:
        "loader reads location.searchStr without loaderDeps — declare " +
        "`loaderDeps: ({ search }) => search` so search-only navigations get " +
        "their own match + loaderData (component-side use: annotate // loader-deps-ok)",
    });
  }
  return hits;
}

const hits = walk(ROOT).flatMap(scan);

if (hits.length === 0) {
  console.log("check-loader-deps: ok");
  process.exit(0);
}

console.error("check-loader-deps: search-consuming loaders without loaderDeps:\n");
for (const h of hits) {
  console.error(`  ${h.file}:${h.line}: ${h.detail}`);
}
console.error(
  "\nMatch ids ignore search params unless loaderDeps declares them — loaders keyed on ?params serve stale data without it.",
);
process.exit(1);
