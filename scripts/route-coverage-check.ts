/**
 * Route coverage gate implementation (invoked by scripts/route-coverage-check.sh).
 *
 * Change-aware mode (UI_COVERAGE_BASE or UI_COVERAGE_TOUCHED): newly added or
 * modified user-facing routes cannot be skip-only — they need happy-dom mount
 * or stack-browser evidence. layoutOnly shells are exempt.
 */
import { existsSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import {
  formatTouchedList,
  resolveUiCoverageDiff,
} from "./ui-coverage-diff.ts";

type Evidence =
  | { kind: "happy-dom"; test: string }
  | { kind: "stack-browser"; test: string }
  | { kind: "skip"; rationale: string };

type Entry = {
  route: string;
  layoutOnly?: boolean;
  coverage: Evidence[];
};

function walkTsrx(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) walkTsrx(full, out);
    else if (name.endsWith(".tsrx")) out.push(full);
  }
  return out;
}

const root = resolve(import.meta.dir, "..");
const routesDir = join(root, "apps/web/src/routes");
const manifestPath = join(root, "apps/web/src/test/route-coverage.manifest.ts");
const uiDiff = resolveUiCoverageDiff(root);
const touchedRoutes = new Set(
  (uiDiff?.touched ?? [])
    .filter((p) => p.startsWith("routes/"))
    .map((p) => p.slice("routes/".length)),
);
const addedRoutes = new Set(
  (uiDiff?.added ?? [])
    .filter((p) => p.startsWith("routes/"))
    .map((p) => p.slice("routes/".length)),
);

if (!existsSync(manifestPath)) {
  console.error(`route-coverage-check: FAIL: missing manifest: ${manifestPath}`);
  process.exit(1);
}
if (!existsSync(routesDir)) {
  console.error(`route-coverage-check: FAIL: missing routes dir: ${routesDir}`);
  process.exit(1);
}

const discovered = walkTsrx(routesDir)
  .map((abs) => relative(routesDir, abs).split("\\").join("/"))
  .sort((a, b) => a.localeCompare(b));

if (discovered.length === 0) {
  console.error(`route-coverage-check: FAIL: no .tsrx routes under ${routesDir}`);
  process.exit(1);
}

const mod = await import(manifestPath);
const manifest = mod.routeCoverageManifest as Entry[];

if (!Array.isArray(manifest)) {
  console.error(
    "route-coverage-check: FAIL: routeCoverageManifest is not an array",
  );
  process.exit(1);
}

const byRoute = new Map<string, Entry>();
const dupes: string[] = [];
for (const entry of manifest) {
  if (!entry?.route || typeof entry.route !== "string") {
    console.error("route-coverage-check: FAIL: entry missing route string");
    process.exit(1);
  }
  if (byRoute.has(entry.route)) dupes.push(entry.route);
  byRoute.set(entry.route, entry);
}

const errors: string[] = [];
if (dupes.length) {
  errors.push(`duplicate manifest routes: ${dupes.join(", ")}`);
}

const missingFromManifest: string[] = [];
const uncovered: string[] = [];
const badEvidence: string[] = [];
const touchedSkipOnly: string[] = [];
let required = 0;
let layoutOnly = 0;
let withHappy = 0;
let withBrowser = 0;
let withSkipOnly = 0;

for (const route of discovered) {
  const entry = byRoute.get(route);
  if (!entry) {
    missingFromManifest.push(route);
    continue;
  }
  if (entry.layoutOnly) {
    layoutOnly += 1;
    continue;
  }
  required += 1;
  const coverage = Array.isArray(entry.coverage) ? entry.coverage : [];
  if (coverage.length === 0) {
    uncovered.push(`${route} (empty coverage[])`);
    continue;
  }

  let okHappy = false;
  let okBrowser = false;
  let okSkip = false;

  for (const c of coverage) {
    if (!c || typeof c !== "object" || !("kind" in c)) {
      badEvidence.push(`${route}: invalid coverage item`);
      continue;
    }
    if (c.kind === "happy-dom") {
      const testPath = resolve(root, c.test);
      if (!c.test || !existsSync(testPath)) {
        badEvidence.push(`${route}: happy-dom test missing: ${c.test}`);
      } else {
        okHappy = true;
      }
    } else if (c.kind === "stack-browser") {
      const testPath = resolve(root, c.test);
      if (!c.test || !existsSync(testPath)) {
        badEvidence.push(`${route}: stack-browser test missing: ${c.test}`);
      } else {
        okBrowser = true;
      }
    } else if (c.kind === "skip") {
      const rationale = String(c.rationale ?? "").trim();
      if (!rationale) {
        badEvidence.push(`${route}: skip missing rationale`);
      } else {
        okSkip = true;
      }
    } else {
      badEvidence.push(
        `${route}: unknown kind ${(c as { kind: string }).kind}`,
      );
    }
  }

  if (!(okHappy || okBrowser || okSkip)) {
    uncovered.push(`${route} (no valid happy-dom / stack-browser / skip)`);
  } else if (okHappy || okBrowser) {
    if (okHappy) withHappy += 1;
    if (okBrowser) withBrowser += 1;
  } else {
    withSkipOnly += 1;
    if (touchedRoutes.has(route)) {
      touchedSkipOnly.push(route);
    }
  }
}

const orphans = [...byRoute.keys()].filter((r) => !discovered.includes(r));
if (orphans.length) {
  errors.push(
    `manifest entries with no route file:\n  - ${orphans.join("\n  - ")}`,
  );
}
if (missingFromManifest.length) {
  errors.push(
    `routes missing from manifest:\n  - ${missingFromManifest.join("\n  - ")}`,
  );
}
if (uncovered.length) {
  errors.push(`routes without valid coverage:\n  - ${uncovered.join("\n  - ")}`);
}
if (badEvidence.length) {
  errors.push(`invalid evidence:\n  - ${badEvidence.join("\n  - ")}`);
}
if (uiDiff && touchedSkipOnly.length) {
  const addedTouched = touchedSkipOnly.filter((r) => addedRoutes.has(r));
  const modifiedTouched = touchedSkipOnly.filter((r) => !addedRoutes.has(r));
  const parts: string[] = [];
  if (addedTouched.length) {
    parts.push(
      `newly added routes must have happy-dom or stack-browser proof (skip not allowed):\n${formatTouchedList(addedTouched)}`,
    );
  }
  if (modifiedTouched.length) {
    parts.push(
      `changed routes are still skip-only — add a happy-dom mount or stack-browser suite before merging:\n${formatTouchedList(modifiedTouched)}`,
    );
  }
  errors.push(parts.join("\n"));
}

const mode = uiDiff
  ? `change-aware base=${uiDiff.base} touched-routes=${touchedRoutes.size}`
  : "inventory-only";
console.log(
  `route-coverage-check: mode=${mode} discovered=${discovered.length} layoutOnly=${layoutOnly} required=${required} happy-dom=${withHappy} stack-browser=${withBrowser} skip-only=${withSkipOnly}`,
);

if (errors.length) {
  for (const e of errors) console.error(`route-coverage-check: FAIL: ${e}`);
  process.exit(1);
}

console.log("route-coverage-check: PASS");
