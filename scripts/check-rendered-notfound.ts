#!/usr/bin/env bun
/**
 * Post-build rendered-output gate for issue #169-class regressions.
 *
 * `astro build` prerenders every page. Dynamic pages must emit a *loading
 * skeleton* — never a "not found" state — because params resolve client-side
 * and every query is still in flight at build time. A page whose shell HTML
 * contains not-found copy either baked the failure in at prerender or lets a
 * leaf fall through to RepoNotFound while pending; both shipped as the #169
 * report. (`404.html` is excluded — it is the real not-found page.)
 *
 * Sibling of scripts/check-rendered-semicolons.ts (issue #165): source-side
 * coverage is scripts/check-pending-before-notfound.ts plus the
 * repo-pending-guard integration suite; this is the all-pages verification
 * at the layer the bug actually appears in.
 *
 * Requires apps/web/dist — run `bun run --filter @oxidean/web build` first.
 * Wired into CI after the web build (web-octane job) via `make web-render-check`.
 */
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..");
const DIST = join(ROOT, "apps", "web", "dist");

// Not-found copy rendered by RepoNotFound plus per-leaf EmptyState/heading
// titles (owner profiles, run/discussion/project detail pages).
const NOT_FOUND_COPY = [
  "Page not found",
  "We couldn't find that page",
  "Organization not found",
  "User not found",
  "Run not found",
  "Discussion not found",
  "Project not found",
];

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (name.endsWith(".html")) out.push(p);
  }
  return out;
}

if (!existsSync(DIST)) {
  console.error(
    "check-rendered-notfound: apps/web/dist is missing — " +
      "run `bun run --filter @oxidean/web build` first.",
  );
  process.exit(1);
}

const hits: { file: string; copy: string }[] = [];
for (const file of walk(DIST)) {
  const rel = relative(ROOT, file);
  if (rel.endsWith("404.html")) continue;
  const html = readFileSync(file, "utf-8");
  for (const copy of NOT_FOUND_COPY) {
    if (html.includes(copy)) hits.push({ file: rel, copy });
  }
}

if (hits.length === 0) {
  console.log("check-rendered-notfound: ok");
  process.exit(0);
}

console.error("check-rendered-notfound: prerendered pages contain not-found copy:\n");
for (const h of hits) {
  console.error(`  ${h.file} — "${h.copy}"`);
}
console.error(
  "\nDynamic shells must prerender a loading skeleton, not a not-found " +
    "state — params resolve client-side, so baked not-found copy flashes on " +
    "every real request (issue #169). Gate leaf queries on isPending before " +
    "rendering RepoNotFound; the repo-pending-guard integration suite covers " +
    "the mount-level invariant.",
);
process.exit(1);
