#!/usr/bin/env bun
/**
 * Three-way route-manifest sync gate.
 *
 * The production serving path is a pair of hand-maintained route corpora:
 *
 *   1. `crates/oxidean-web/src/shells.rs` `SHELLS` — request path → dist shell
 *      (which shell file, which params, whether anonymous → login).
 *   2. `apps/web/src/pages/**` — Astro pages; each emits one dist shell at
 *      `{route}/index.html` with `[param]`/`[...rest]` positions emitted as `_`.
 *   3. `apps/web/src/lib/route-params.ts` `RESERVED_TOP_SEGMENTS` /
 *      `RESERVED_OWNER_SEGMENTS` — the client-side guard that keeps
 *      `/{owner}`/`/{owner}/{repo}` params from misbinding app routes.
 *
 * Drift in either direction is silent and expensive: a new page without a
 * SHELLS entry 404s in production (tests pass — vitest renders components,
 * not the dispatch table), and a stale SHELLS entry serves an old shell or
 * 404s. This check fails when:
 *
 *   - a page emits a dist file no SHELLS entry serves (missing dispatch), or
 *   - a SHELLS entry points at a dist file no page emits (dangling entry), or
 *   - a SHELLS pattern's param names disagree with the page's `[param]` names
 *     (title interpolation and island `matchPath` read different names), or
 *   - a literal top-level/`{owner}/<lit>` route segment is missing from the
 *     RESERVED sets (client repo-context binding would treat it as a repo).
 *
 * Emitted-file comparison, not pattern-equality: SHELLS aliases like
 * `/settings` → `settings/general/index.html` are legal; what matters is that
 * every served file exists and every emitted file is served.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "..");
const SHELLS_RS = join(REPO, "crates", "oxidean-web", "src", "shells.rs");
const PAGES_DIR = join(REPO, "apps", "web", "src", "pages");
const ROUTE_PARAMS = join(REPO, "apps", "web", "src", "lib", "route-params.ts");

type Seg = { kind: "lit" | "param" | "splat"; value: string };
type ShellEntry = { pattern: Seg[]; file: string; protected: boolean };

// ── Parse SHELLS from shells.rs ─────────────────────────────────────────────

const shellsSrc = readFileSync(SHELLS_RS, "utf8");
const entries: ShellEntry[] = [];
// Entries may be single-line or rustfmt-expanded multi-line; `title` strings
// contain `{param}` braces so brace-balanced matching is unsafe — anchor on the
// rustfmt `},` terminator between entries instead.
for (const m of shellsSrc.matchAll(/Shell\s*\{(.*?)\}\s*,/gs)) {
  const body = m[1]!;
  const pattern = body.match(/pattern:\s*&\[([^\]]*)\]/);
  const file = body.match(/file:\s*"([^"]+)"/);
  const prot = body.match(/protected:\s*(true|false)/);
  if (!pattern || !file || !prot) continue;
  const segs: Seg[] = [];
  for (const s of pattern[1]!.matchAll(/([LPS])\("([^"]+)"\)/g)) {
    segs.push({
      kind: s[1] === "L" ? "lit" : s[1] === "P" ? "param" : "splat",
      value: s[2]!,
    });
  }
  entries.push({ pattern: segs, file: file[1]!, protected: prot[1] === "true" });
}
if (entries.length === 0) {
  console.error("check-route-sync: parsed zero SHELLS entries — shells.rs format changed?");
  process.exit(1);
}

// ── Enumerate Astro pages → emitted dist files + page route shapes ─────────

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (name.endsWith(".astro")) out.push(p);
  }
  return out;
}

type Page = { route: Seg[]; distFile: string; rel: string };

const pages: Page[] = walk(PAGES_DIR).map((abs) => {
  const rel = abs.slice(PAGES_DIR.length + 1);
  const noExt = rel.replace(/\.astro$/, "").replace(/(^|\/)index$/, "");
  const route: Seg[] = noExt
    .split("/")
    .filter(Boolean)
    .map((seg) => {
      const rest = seg.match(/^\[\.\.\.([^\]]+)\]$/);
      if (rest) return { kind: "splat", value: rest[1]! };
      const param = seg.match(/^\[([^\]]+)\]$/);
      if (param) return { kind: "param", value: param[1]! };
      return { kind: "lit", value: seg };
    });
  // Astro emits each page at {route}/index.html with param positions as `_`.
  // `404.astro` and the root `index.astro` emit flat files instead.
  let distFile: string;
  if (noExt === "404") distFile = "404.html";
  else if (route.length === 0) distFile = "index.html";
  else distFile = `${route.map((s) => (s.kind === "lit" ? s.value : "_")).join("/")}/index.html`;
  return { route, distFile, rel };
});

// ── Invariant 1: emitted files == served files ──────────────────────────────

const served = new Set(entries.map((e) => e.file));
served.add("index.html"); // dispatch handles `/` outside the table
const emitted = new Set(pages.map((p) => p.distFile));

const problems: string[] = [];
for (const f of [...served].filter((f) => f !== "404.html" && !emitted.has(f))) {
  problems.push(`SHELLS serves ${f} but no page emits it (dangling dispatch entry)`);
}
for (const f of emitted) {
  if (f === "404.html") continue; // served by not_found, not the table
  if (!served.has(f)) {
    problems.push(`page emits ${f} but SHELLS has no entry — production requests 404`);
  }
}

// ── Invariant 2: param names agree where a SHELLS pattern maps to a page ────

function routeKey(segs: Seg[]): string {
  return segs.map((s) => (s.kind === "lit" ? s.value : s.kind === "param" ? "*" : "**")).join("/");
}
const pageByRouteKey = new Map(pages.map((p) => [routeKey(p.route), p]));

for (const e of entries) {
  const page = pageByRouteKey.get(routeKey(e.pattern));
  if (!page) continue; // alias entries (e.g. /settings) share another page's file
  for (let i = 0; i < e.pattern.length; i++) {
    const s = e.pattern[i]!;
    const r = page.route[i]!;
    if (s.kind !== "lit" && s.value !== r.value) {
      problems.push(
        `/${e.pattern.map((x) => (x.kind === "lit" ? x.value : `{${x.value}}`)).join("/")}: ` +
          `param "${s.value}" mismatches page ${page.rel} param "${r.value}"`,
      );
    }
  }
}

// ── Invariant 3: RESERVED segment coverage ──────────────────────────────────

const rpSrc = readFileSync(ROUTE_PARAMS, "utf8");
function parseSet(name: string): Set<string> {
  const m = rpSrc.match(new RegExp(`${name}[^=]*=\\s*new Set\\(\\[([^\\]]*)\\]`));
  if (!m) {
    console.error(`check-route-sync: could not parse ${name} in route-params.ts`);
    process.exit(1);
  }
  return new Set([...m[1]!.matchAll(/"([^"]+)"/g)].map((x) => x[1]!));
}
const topReserved = parseSet("RESERVED_TOP_SEGMENTS");
const ownerReserved = parseSet("RESERVED_OWNER_SEGMENTS");

for (const e of entries) {
  const first = e.pattern[0]!;
  if (first.kind === "lit" && !topReserved.has(first.value)) {
    problems.push(
      `route /${first.value}/… is a literal SHELLS top segment but "${first.value}" ` +
        `is not in RESERVED_TOP_SEGMENTS — islands would misbind it as {owner}`,
    );
  }
  const [a, b] = e.pattern;
  if (a?.kind === "param" && b?.kind === "lit" && !ownerReserved.has(b.value)) {
    problems.push(
      `/{owner}/${b.value}… literal SHELLS segment not in RESERVED_OWNER_SEGMENTS — ` +
        `islands would misbind it as {repo}`,
    );
  }
}
// Reverse: a reserved segment with no route is dead weight (warn, not fail).
const literalsTop = new Set(
  entries.filter((e) => e.pattern[0]!.kind === "lit").map((e) => e.pattern[0]!.value),
);
for (const seg of topReserved) {
  if (!literalsTop.has(seg) && !["dashboard", "api"].includes(seg)) {
    console.warn(`check-route-sync: warn — RESERVED_TOP_SEGMENTS "${seg}" has no route`);
  }
}

if (problems.length === 0) {
  console.log(`check-route-sync: ok (${entries.length} shells ↔ ${pages.length} pages)`);
  process.exit(0);
}
console.error("check-route-sync: route manifest drift:\n");
for (const p of problems) console.error(`  ${p}`);
console.error(
  "\nUpdate crates/oxidean-web/src/shells.rs and/or apps/web/src/pages/** together; " +
    "reserved sets live in apps/web/src/lib/route-params.ts.",
);
process.exit(1);
