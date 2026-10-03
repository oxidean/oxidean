#!/usr/bin/env bun
/**
 * Programmatic internal navigation must go through the app router —
 * `useAppNavigate()` (apps/web/src/lib/app-navigate.ts) or an <AppLink> —
 * never `window.location.assign` / `location.href =`, which force a full
 * document reload (re-fetch HTML, re-parse JS, blank flash). Sibling of
 * check-internal-anchors.ts, which covers declarative `<a>` tags.
 *
 * This scan flags every `location.assign(`, `location.replace(`, and
 * `location.href =` in apps/web sources (.ts/.tsx/.tsrx, tests excluded —
 * test files legitimately spy on these APIs). Legitimate full-document
 * navigations are opted out with a `raw-nav-ok` marker on the line or up to
 * 3 lines above:
 *
 *   - session-creating/destroying transitions (post-login/signup/reset/
 *     setup, logout, factory reset) where a clean reload is deliberate
 *   - /api/ endpoints and downloads (archive zip/tar, SSO start) — not routes
 *   - code running outside the router context (boot guards)
 *
 * Reads (`location.hash` / `.search` / `.origin`) and `location.reload()`
 * (service-worker update flow) never flag.
 *
 * Example: window.location.assign("/api/auth/workos/start"); // raw-nav-ok — SSO endpoint
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..", "apps", "web", "src");
const OPT_OUT = /raw-nav-ok/;
// Assigning location.href or calling location.assign/replace. `location` must
// not be a property of another object member expression beyond `window.`.
const RAW_NAV =
  /(?:\bwindow\.)?\blocation\.(?:assign|replace)\s*\(|(?:\bwindow\.)?\blocation\.href\s*=/g;

type Hit = { file: string; line: number; detail: string };

function isSourceFile(name: string): boolean {
  if (/\.test\.[a-z]+$/.test(name)) return false;
  return name.endsWith(".tsrx") || name.endsWith(".ts") || name.endsWith(".tsx");
}

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (isSourceFile(name)) out.push(p);
  }
  return out;
}

/** Strip // line comments so doc/comment mentions of the API never flag. */
function stripLineComments(src: string): string {
  return src.replace(/\/\/[^\n]*/g, (m) => " ".repeat(m.length));
}

function scan(file: string): Hit[] {
  const rel = relative(join(import.meta.dir, ".."), file);
  const src = readFileSync(file, "utf8");
  const code = stripLineComments(src);
  const lines = src.split(/\r?\n/);
  const hits: Hit[] = [];

  let m: RegExpExecArray | null;
  RAW_NAV.lastIndex = 0;
  while ((m = RAW_NAV.exec(code))) {
    const pos = m.index;
    const line = code.slice(0, pos).split("\n").length;
    // Opt-out window: marker on the call's line/statement or up to 3 lines
    // above (covers `// raw-nav-ok — reason` and block comments).
    const callSlice = code.slice(pos, pos + 400);
    const stmtEnd = callSlice.indexOf(";");
    const window =
      `${lines[line - 4] ?? ""}\n${lines[line - 3] ?? ""}\n${lines[line - 2] ?? ""}\n` +
      callSlice.slice(0, stmtEnd > 0 ? stmtEnd + 1 : 400);
    if (OPT_OUT.test(window)) continue;
    hits.push({
      file: rel,
      line,
      detail:
        "full-document navigation — use useAppNavigate() (@/lib/app-navigate) or " +
        "<AppLink> for internal routes; keep reloads only for session-changing " +
        "transitions, /api/ downloads, or non-router code: annotate // raw-nav-ok",
    });
  }
  return hits;
}

const files = walk(ROOT);
const hits = files.flatMap(scan);

if (hits.length === 0) {
  console.log("check-internal-nav: ok");
  process.exit(0);
}

console.error("check-internal-nav: full-document navigations found:\n");
for (const h of hits) {
  console.error(`  ${h.file}:${h.line}: ${h.detail}`);
}
console.error(
  "\nUse useAppNavigate() from @/lib/app-navigate (or <AppLink>) for internal navigation, or annotate intentional full reloads with // raw-nav-ok.",
);
process.exit(1);
