#!/usr/bin/env bun
/**
 * Internal navigation must use the shared <AppLink> component
 * (apps/web/src/components/ui/app-link.tsrx), which renders a router-aware
 * anchor: client-side transitions + intent preloading in-app, and a plain
 * <a> fallback in RouterProvider-free test harnesses. A raw <a href="…">
 * performs a full document reload — visible as theme/layout flicker.
 *
 * This scan flags every raw `<a` opening tag in apps/web .tsrx sources that
 * carries an `href` attribute. Legitimate raw anchors are opted out with a
 * `raw-anchor-ok` marker on the tag, the line above, or in a nearby comment:
 *
 *   - external URLs (homepage links, CI status target_url, mailto:)
 *   - downloads / non-route endpoints (/api/…, raw blob fetches)
 *   - #same-page hash anchors
 *   - anchors without href (name/link targets) never flag
 *
 * Example: <a href={externalUrl} target="_blank" rel="noreferrer"> // raw-anchor-ok
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..", "apps", "web", "src");
const OPT_OUT = /raw-anchor-ok/;
// `<a` opening tag (lowercase only — <A…> components and <AppLink> exempt).
const ANCHOR_OPEN = /<a(?=[\s/>])/g;

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

/** Does the opening tag starting at `offset` carry an href attribute? */
function tagHasHref(src: string, offset: number): boolean {
  // Scan to the tag's closing '>' with brace/quote awareness.
  let depth = 0;
  let quote: string | null = null;
  for (let j = offset; j < src.length && j < offset + 4000; j++) {
    const c = src[j]!;
    if (quote) {
      if (c === quote && src[j - 1] !== "\\") quote = null;
    } else if (c === '"' || c === "'" || c === "`") {
      quote = c;
    } else if (c === "{") {
      depth++;
    } else if (c === "}") {
      depth--;
    } else if (c === ">" && depth === 0) {
      return /\bhref\s*=/.test(src.slice(offset, j));
    }
  }
  return false;
}

function scan(file: string): Hit[] {
  const rel = relative(join(import.meta.dir, ".."), file);
  const src = readFileSync(file, "utf8");
  const lines = src.split(/\r?\n/);
  const hits: Hit[] = [];

  let m: RegExpExecArray | null;
  ANCHOR_OPEN.lastIndex = 0;
  while ((m = ANCHOR_OPEN.exec(src))) {
    const pos = m.index;
    const line = src.slice(0, pos).split("\n").length;
    if (!tagHasHref(src, pos)) continue;
    // Opt-out window: marker in the tag's own source slice, on its line, or up
    // to 3 lines above (covers `// raw-anchor-ok` and `{/* raw-anchor-ok */}`).
    const tagSlice = src.slice(pos, pos + 4000);
    const tagEnd = tagSlice.indexOf(">");
    const window =
      `${lines[line - 4] ?? ""}\n${lines[line - 3] ?? ""}\n${lines[line - 2] ?? ""}\n` +
      tagSlice.slice(0, tagEnd > 0 ? tagEnd + 1 : 400);
    if (OPT_OUT.test(window)) continue;
    hits.push({
      file: rel,
      line,
      detail:
        "raw <a href> — internal navigation must use <AppLink> for client-side " +
        "routing (full reloads flash theme/layout); external/download/hash links: " +
        "annotate // raw-anchor-ok",
    });
  }
  return hits;
}

const files = walk(ROOT);
const hits = files.flatMap(scan);

if (hits.length === 0) {
  console.log("check-internal-anchors: ok");
  process.exit(0);
}

console.error("check-internal-anchors: raw <a href> anchors found:\n");
for (const h of hits) {
  console.error(`  ${h.file}:${h.line}: ${h.detail}`);
}
console.error(
  "\nUse <AppLink> from @/components/ui/app-link for internal navigation, or annotate intentional raw anchors with // raw-anchor-ok.",
);
process.exit(1);
