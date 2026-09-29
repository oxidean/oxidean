#!/usr/bin/env bun
/**
 * Heuristic scan for Octane/Base UI DOM races (insertBefore /
 * HierarchyRequestError "Something went wrong!" overlays):
 *
 * 1. After </RadioGroup>, flag nearby @if … @else sibling swaps.
 * 2. Flag `createPortal(` — app code must not portal manually.
 * 3. Flag `<*Portal>` JSX (DialogPortal, AlertDialogPortal, SelectPortal,
 *    MenuPrimitive.Portal, Toast.Portal, FloatingPortal, …) whose opening tag
 *    lacks `keepMounted` — unmounted portals change root-node count mid
 *    reconciliation and race sibling updates.
 * 4. Flag `@if` / `@else` / `@for` nested inside `form.Subscribe` /
 *    `*.Subscribe` render-prop children (not bare `form.Field` — per-field
 *    error `@if`s are common and lower risk). Conditional mount while a
 *    Subscribe re-renders races Base UI Indicator mount/unmount.
 * 5. Flag `onCheckedChange` / `onValueChange` that call `form.setFieldValue`
 *    inside a `form.Subscribe` whose selector is a broad `s.values` (or
 *    equivalent full-values subscribe).
 *
 * Prefer keeping both panels mounted (`hidden` class) — see .agents/skills/octane/SKILL.md.
 *
 * Opt out a site with: // octane-dom-race-ok
 * on the flagged line, the line above, or inside the portal's opening tag —
 * e.g. anchor-scoped popups that cannot keepMount and were reviewed.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..", "apps", "web", "src");
const WINDOW = 50;
const OPT_OUT = /octane-dom-race-ok/;
// `<FooPortal>` / `<Foo.Portal>` opening tags (not closing `</…Portal>`).
const PORTAL_TAG = /<[A-Z][A-Za-z0-9.]*Portal(?=[\s>/])|<Portal(?=[\s>/])/;
// `form.Subscribe` / `foo.Subscribe` only — not `form.Field`.
const SUBSCRIBE_OPEN = /<(?:form\.)?Subscribe\b|<([A-Za-z_][\w.]*)\.Subscribe\b/;
const BROAD_VALUES_SELECTOR =
  /selector=\{\s*\(\s*s\s*\)\s*=>\s*s\.values\s*\}|selector=\{\s*\(\s*\{\s*values\s*\}\s*\)\s*=>\s*values\s*\}/;

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

function optedOut(lines: string[], index: number, endExclusive?: number): boolean {
  const end = endExclusive ?? index + 1;
  const region = lines.slice(Math.max(0, index - 3), end).join("\n");
  return OPT_OUT.test(region);
}

function scan(file: string): Hit[] {
  const rel = relative(join(import.meta.dir, ".."), file);
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  const hits: Hit[] = [];

  for (let i = 0; i < lines.length; i++) {
    if (!lines[i]!.includes("</RadioGroup>")) continue;

    const end = Math.min(lines.length, i + 1 + WINDOW);
    let ifLine = -1;
    let ifIndent = -1;
    for (let j = i + 1; j < end; j++) {
      const line = lines[j]!;
      if (line.includes("</RadioGroup>")) break;

      const ifMatch = line.match(/^(\s*)@if\s*\(/);
      if (ifMatch && ifLine < 0) {
        ifLine = j;
        ifIndent = ifMatch[1]!.length;
        continue;
      }
      if (ifLine < 0) continue;

      const elseMatch = line.match(/^(\s*)\}\s*@else\s*\{/);
      if (elseMatch && elseMatch[1]!.length === ifIndent) {
        const region = lines.slice(ifLine, j + 1).join("\n");
        if (/octane-dom-race-ok/.test(region)) break;
        hits.push({
          file: rel,
          line: ifLine + 1,
          detail:
            "`</RadioGroup>` followed by sibling `@if`/`@else` within " +
            `${WINDOW} lines — prefer CSS hidden panels or // octane-dom-race-ok`,
        });
        break;
      }
    }
  }
  return hits;
}

function scanPortals(file: string): Hit[] {
  const rel = relative(join(import.meta.dir, ".."), file);
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  const hits: Hit[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    // Opt-out comments may sit up to 3 lines above the portal tag (multi-line
    // `//` or `{/* */}` blocks) or on the same/next line inside the tag.
    const window =
      `${lines[i - 3] ?? ""}\n${lines[i - 2] ?? ""}\n${lines[i - 1] ?? ""}\n` +
      `${line}\n${lines[i + 1] ?? ""}`;

    if (line.includes("createPortal(") && !OPT_OUT.test(window)) {
      hits.push({
        file: rel,
        line: i + 1,
        detail:
          "`createPortal(` in app code — portals change root-node count mid " +
          "reconciliation; render in-tree or // octane-dom-race-ok",
      });
      continue;
    }

    if (!PORTAL_TAG.test(line)) continue;
    // Opening tag may span two lines — accept keepMounted in the tag window.
    if (/keepMounted/.test(`${line}\n${lines[i + 1] ?? ""}`)) continue;
    if (OPT_OUT.test(window)) continue;
    hits.push({
      file: rel,
      line: i + 1,
      detail:
        "portal JSX without `keepMounted` — unmounted portals race Octane " +
        "sibling reconciliation (insertBefore); add keepMounted or // octane-dom-race-ok",
    });
  }
  return hits;
}

/** Match `</form.Subscribe>` / `</acceptForm.Subscribe>` / `</Subscribe>`. */
const SUBSCRIBE_CLOSE = /<\/(?:[A-Za-z_][\w]*)?\.?Subscribe\s*>|<\/Subscribe\s*>/;

/** Find the line index of the matching Subscribe close tag (nesting-aware). */
function findSubscribeClose(lines: string[], openLine: number): number {
  let depth = 0;
  for (let i = openLine; i < lines.length; i++) {
    const line = lines[i]!;
    // Count opens/closes on the line (rare to have both).
    const opens = line.match(/<(?:form\.)?Subscribe\b|<([A-Za-z_][\w]*)\.Subscribe\b/g);
    const closes = line.match(/<\/(?:[A-Za-z_][\w]*)?\.?Subscribe\s*>|<\/Subscribe\s*>/g);
    if (opens) depth += opens.length;
    if (closes) {
      depth -= closes.length;
      if (depth <= 0) return i;
    }
  }
  return Math.min(lines.length - 1, openLine + 400);
}

function scanSubscribeControlFlow(file: string): Hit[] {
  const rel = relative(join(import.meta.dir, ".."), file);
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  const hits: Hit[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (!SUBSCRIBE_OPEN.test(line)) continue;
    if (SUBSCRIBE_CLOSE.test(line)) continue; // closing tag
    if (/\/>\s*$/.test(line.trim())) continue;

    const end = findSubscribeClose(lines, i);
    // Whole-Subscribe opt-out: marker on/above the opening tag or anywhere in body.
    if (optedOut(lines, i, end + 1)) continue;
    for (let k = i + 1; k < end; k++) {
      const body = lines[k]!;
      if (!/@(if|else|for)\b/.test(body)) continue;
      if (optedOut(lines, k)) continue;
      hits.push({
        file: rel,
        line: k + 1,
        detail:
          "`@if`/`@else`/`@for` inside form.Subscribe / *.Subscribe " +
          "render-prop — keep nodes mounted (`hidden`) or extract a child component; " +
          "or // octane-dom-race-ok",
      });
      break;
    }
  }
  return hits;
}

function scanBroadSubscribeMutations(file: string): Hit[] {
  const rel = relative(join(import.meta.dir, ".."), file);
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  const hits: Hit[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (!SUBSCRIBE_OPEN.test(line)) continue;
    if (SUBSCRIBE_CLOSE.test(line)) continue;
    // Look at this line + next 2 for broad selector.
    const head = `${line}\n${lines[i + 1] ?? ""}\n${lines[i + 2] ?? ""}`;
    if (!BROAD_VALUES_SELECTOR.test(head)) continue;

    const end = findSubscribeClose(lines, i);
    if (optedOut(lines, i, end + 1)) continue;
    const region = lines.slice(i, end + 1).join("\n");
    if (!/on(Checked|Value)Change\s*=/.test(region)) continue;
    if (!/form\.setFieldValue\s*\(/.test(region)) continue;
    hits.push({
      file: rel,
      line: i + 1,
      detail:
        "`onCheckedChange`/`onValueChange` calling `form.setFieldValue` inside " +
        "`Subscribe` of full `s.values` — narrow the selector / use form.Field, " +
        "or // octane-dom-race-ok",
    });
  }
  return hits;
}

const files = walk(ROOT);
const hits = files.flatMap((f) => [
  ...scan(f),
  ...scanPortals(f),
  ...scanSubscribeControlFlow(f),
  ...scanBroadSubscribeMutations(f),
]);

if (hits.length === 0) {
  console.log("check-octane-dom-races: ok");
  process.exit(0);
}

console.error("check-octane-dom-races: potential Octane/Base UI DOM races:\n");
for (const h of hits) {
  console.error(`  ${h.file}:${h.line}: ${h.detail}`);
}
console.error(
  "\nSee .agents/skills/octane/SKILL.md (insertBefore failure mode) and apps/web/src/test/dom-errors.ts.",
);
process.exit(1);
