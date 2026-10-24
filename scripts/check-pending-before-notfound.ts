#!/usr/bin/env bun
/**
 * Pending-before-not-found source gate for issue #169-class regressions.
 *
 * The bug shape that shipped #169 (and #120 before it): a leaf's not-found
 * guard is a boolean like
 *
 *   const showNotFound = !data || data.kind === "not_found" || …
 *
 * where `!data` is true while the query is still in flight — RepoNotFound
 * renders for the whole pending window instead of the skeleton. Safe shapes:
 *
 *   - gate pending inside the expression: `(!q.isPending && !data) || …`
 *   - gate pending in template order: `@if (q.isPending)` before the
 *     `@else if (showNotFound)` that renders RepoNotFound
 *   - phase unions that default to `{ kind: "loading" }`
 *
 * This check scans `*otFound`-named const initializers in `.tsrx` sources for
 * bare `!data`/`!loader`-style negations and requires a pending token
 * (`isPending`, `isLoading`, `"loading"`) somewhere in the same file —
 * matching how the codebase already documents dominance (watchers.tsrx's
 * `@if (dataQ.isPending …)` guard). Opt out of a false positive (e.g.
 * provider state that isn't a query) with `// pending-guard-ok` on the const
 * line or the line above it.
 *
 * Complements, not replaces, the semantic gate: the repo-pending-guard
 * integration suite mounts every repo leaf on permanently-pending queries
 * and asserts no not-found copy renders — it covers not-found branches this
 * syntactic check can't reach (e.g. `@else if (!release)` inline).
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dir, "..");
const SRC = join(ROOT, "apps", "web", "src");

const NOTFOUND_DECL =
  /const\s+(\w*(?:show)?not_?found\w*|missing\w*|gone\w*)\s*=\s*([\s\S]*?);/gi;
// Bare negations of the conventional leaf-loader data names — `!x` that is
// true while the query is pending is the #169 idiom. `!!x` (Boolean coercion,
// "x is set") is excluded via the lookbehind: it can never be the pending
// flash. `!x === y` / `!x?.y` comparisons don't match: `!` must bind the bare
// identifier.
const BARE_NEG =
  /(?<!!)!(?:data|loader|loaderData|routeLoader|repo|issue|pull|release|run|milestone|discussion|project|commit|list|result|item)\b/;
const PENDING_TOKEN = /\bisPending\b|\bisLoading\b|["'`]loading["'`]/;
const OPT_OUT = /pending-guard-ok/;

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out);
    else if (name.endsWith(".tsrx")) out.push(p);
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

const hits: { file: string; line: number; name: string }[] = [];
for (const file of walk(SRC)) {
  const src = readFileSync(file, "utf-8");
  const hasPending = PENDING_TOKEN.test(src);
  for (const m of src.matchAll(NOTFOUND_DECL)) {
    const name = m[1]!;
    const init = m[2]!;
    if (!BARE_NEG.test(init)) continue;
    if (hasPending) continue;
    const line = lineOf(src, m.index!);
    const lines = src.split("\n");
    if (OPT_OUT.test(lines[line - 1] ?? "") || OPT_OUT.test(lines[line - 2] ?? "")) continue;
    hits.push({ file: relative(ROOT, file), line, name });
  }
}

if (hits.length === 0) {
  console.log("check-pending-before-notfound: ok");
  process.exit(0);
}

console.error(
  "check-pending-before-notfound: not-found guards negate possibly-pending data:\n",
);
for (const h of hits) {
  console.error(`  ${h.file}:${h.line} (${h.name})`);
}
console.error(
  "\nA bare `!data`/`!loader` in a not-found guard is true while the query is " +
    "in flight — the leaf flashes RepoNotFound instead of its skeleton (#169). " +
    "Gate pending first: `(!q.isPending && !data) || …`, or an `@if " +
    "(q.isPending)` skeleton branch before the not-found branch. If the " +
    "negated value isn't query-backed, mark it `// pending-guard-ok`.",
);
process.exit(1);
