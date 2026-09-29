/**
 * Browser (Chromium component) coverage gate.
 * Invoked by scripts/browser-coverage-check.sh / make browser-coverage-check.
 *
 * Discovers high-risk interactive `.tsrx` (Checkbox / RadioGroup / form.Subscribe)
 * and requires a manifest entry with browser, stack-browser, or skip evidence.
 * Browser evidence must prove mount + interaction + overlay/DOM-race asserts.
 *
 * Change-aware mode (UI_COVERAGE_BASE or UI_COVERAGE_TOUCHED): newly added or
 * modified high-risk surfaces cannot be skip-only — they need browser or
 * stack-browser proof. Inventory-only mode still allows bootstrap skips.
 */
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import {
  formatTouchedList,
  resolveUiCoverageDiff,
} from "./ui-coverage-diff.ts";

type Evidence =
  | { kind: "browser"; test: string; subject: string }
  | { kind: "stack-browser"; test: string; subject?: string }
  | { kind: "skip"; rationale: string };

type Entry = {
  surface: string;
  coverage: Evidence[];
};

const HIGH_RISK_RE = /\bCheckbox\b|\bRadioGroup(?:Item)?\b|\bform\.Subscribe\b/;
const EXCLUDE_RE = /\.(?:browser-)?harness\.tsrx$|\.test\./;

const MOUNT_RE = /\bmount(?:Component|WithQueryClient)\b/;
const CLICK_RE = /\bclick(?:TestId|AriaLabel)\b|\.click\s*\(/;
const ASSERT_RE =
  /\bexpectNoOctaneOverlayInDocument\b|\bexpectNoDomRaces\b|\btrackDomErrors\b/;

function walkTsrx(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      if (name === "node_modules" || name === "dist" || name === "coverage") continue;
      walkTsrx(full, out);
    } else if (name.endsWith(".tsrx")) {
      out.push(full);
    }
  }
  return out;
}

const root = resolve(import.meta.dir, "..");
const srcDir = join(root, "apps/web/src");
const manifestPath = join(root, "apps/web/src/test/browser-coverage.manifest.ts");
const uiDiff = resolveUiCoverageDiff(root);

if (!existsSync(manifestPath)) {
  console.error(`browser-coverage-check: FAIL: missing manifest: ${manifestPath}`);
  process.exit(1);
}
if (!existsSync(srcDir)) {
  console.error(`browser-coverage-check: FAIL: missing src dir: ${srcDir}`);
  process.exit(1);
}

const discovered = walkTsrx(srcDir)
  .filter((abs) => {
    const rel = relative(srcDir, abs).split("\\").join("/");
    if (EXCLUDE_RE.test(rel)) return false;
    const src = readFileSync(abs, "utf8");
    return HIGH_RISK_RE.test(src);
  })
  .map((abs) => relative(srcDir, abs).split("\\").join("/"))
  .sort((a, b) => a.localeCompare(b));

if (discovered.length === 0) {
  console.error("browser-coverage-check: FAIL: no high-risk .tsrx discovered");
  process.exit(1);
}

const mod = await import(manifestPath);
const manifest = mod.browserCoverageManifest as Entry[];

if (!Array.isArray(manifest)) {
  console.error(
    "browser-coverage-check: FAIL: browserCoverageManifest is not an array",
  );
  process.exit(1);
}

const bySurface = new Map<string, Entry>();
const dupes: string[] = [];
for (const entry of manifest) {
  if (!entry?.surface || typeof entry.surface !== "string") {
    console.error("browser-coverage-check: FAIL: entry missing surface string");
    process.exit(1);
  }
  if (bySurface.has(entry.surface)) dupes.push(entry.surface);
  bySurface.set(entry.surface, entry);
}

const errors: string[] = [];
if (dupes.length) {
  errors.push(`duplicate manifest surfaces: ${dupes.join(", ")}`);
}

const missingFromManifest: string[] = [];
const uncovered: string[] = [];
const badEvidence: string[] = [];
const touchedSkipOnly: string[] = [];
let withBrowser = 0;
let withStack = 0;
let withSkipOnly = 0;
const touchedSet = new Set(uiDiff?.touched ?? []);
const addedSet = new Set(uiDiff?.added ?? []);

function proveBrowserTest(testRel: string, subject: string, surface: string): void {
  const testPath = resolve(root, testRel);
  if (!testRel || !existsSync(testPath)) {
    badEvidence.push(`${surface}: browser test missing: ${testRel}`);
    return;
  }
  if (!/\.browser\.test\.(ts|tsx)$/.test(testRel)) {
    badEvidence.push(
      `${surface}: browser evidence must be *.browser.test.ts(x), got ${testRel}`,
    );
    return;
  }
  const body = readFileSync(testPath, "utf8");
  if (!subject || !body.includes(subject)) {
    badEvidence.push(
      `${surface}: browser test ${testRel} missing subject marker "${subject}"`,
    );
  }
  if (!MOUNT_RE.test(body)) {
    badEvidence.push(
      `${surface}: browser test ${testRel} must call mountComponent or mountWithQueryClient`,
    );
  }
  if (!CLICK_RE.test(body)) {
    badEvidence.push(
      `${surface}: browser test ${testRel} must interact (clickTestId / clickAriaLabel / .click)`,
    );
  }
  if (!ASSERT_RE.test(body)) {
    badEvidence.push(
      `${surface}: browser test ${testRel} must assert no overlay/DOM race (expectNoOctaneOverlayInDocument / expectNoDomRaces / trackDomErrors)`,
    );
  }
}

for (const surface of discovered) {
  const entry = bySurface.get(surface);
  if (!entry) {
    missingFromManifest.push(surface);
    continue;
  }
  const coverage = Array.isArray(entry.coverage) ? entry.coverage : [];
  if (coverage.length === 0) {
    uncovered.push(`${surface} (empty coverage[])`);
    continue;
  }

  let okBrowser = false;
  let okStack = false;
  let okSkip = false;

  for (const c of coverage) {
    if (!c || typeof c !== "object" || !("kind" in c)) {
      badEvidence.push(`${surface}: invalid coverage item`);
      continue;
    }
    if (c.kind === "browser") {
      proveBrowserTest(c.test, c.subject, surface);
      if (
        c.test &&
        c.subject &&
        existsSync(resolve(root, c.test)) &&
        /\.browser\.test\.(ts|tsx)$/.test(c.test)
      ) {
        const body = readFileSync(resolve(root, c.test), "utf8");
        if (
          body.includes(c.subject) &&
          MOUNT_RE.test(body) &&
          CLICK_RE.test(body) &&
          ASSERT_RE.test(body)
        ) {
          okBrowser = true;
        }
      }
    } else if (c.kind === "stack-browser") {
      const testPath = resolve(root, c.test);
      if (!c.test || !existsSync(testPath)) {
        badEvidence.push(`${surface}: stack-browser test missing: ${c.test}`);
      } else if (!c.test.includes("stack-browser")) {
        badEvidence.push(
          `${surface}: stack-browser evidence path must include stack-browser: ${c.test}`,
        );
      } else {
        const body = readFileSync(testPath, "utf8");
        if (c.subject && !body.includes(c.subject)) {
          badEvidence.push(
            `${surface}: stack-browser test ${c.test} missing subject marker "${c.subject}"`,
          );
        } else {
          okStack = true;
        }
      }
    } else if (c.kind === "skip") {
      const rationale = String(c.rationale ?? "").trim();
      if (!rationale) {
        badEvidence.push(`${surface}: skip missing rationale`);
      } else {
        okSkip = true;
      }
    } else {
      badEvidence.push(
        `${surface}: unknown kind ${(c as { kind: string }).kind}`,
      );
    }
  }

  if (!(okBrowser || okStack || okSkip)) {
    uncovered.push(`${surface} (no valid browser / stack-browser / skip)`);
  } else if (okBrowser || okStack) {
    if (okBrowser) withBrowser += 1;
    if (okStack) withStack += 1;
  } else {
    withSkipOnly += 1;
    if (touchedSet.has(surface)) {
      touchedSkipOnly.push(surface);
    }
  }
}

const orphans = [...bySurface.keys()].filter((s) => !discovered.includes(s));
if (orphans.length) {
  errors.push(
    `manifest entries with no high-risk surface file:\n  - ${orphans.join("\n  - ")}`,
  );
}
if (missingFromManifest.length) {
  errors.push(
    `high-risk surfaces missing from manifest (add *.browser.test.tsx — skip not enough for new UI):\n  - ${missingFromManifest.join("\n  - ")}`,
  );
}
if (uncovered.length) {
  errors.push(`surfaces without valid coverage:\n  - ${uncovered.join("\n  - ")}`);
}
if (badEvidence.length) {
  errors.push(`invalid evidence:\n  - ${badEvidence.join("\n  - ")}`);
}
if (uiDiff && touchedSkipOnly.length) {
  const addedTouched = touchedSkipOnly.filter((s) => addedSet.has(s));
  const modifiedTouched = touchedSkipOnly.filter((s) => !addedSet.has(s));
  const parts: string[] = [];
  if (addedTouched.length) {
    parts.push(
      `newly added high-risk UI must have browser or stack-browser proof (skip not allowed):\n${formatTouchedList(addedTouched)}`,
    );
  }
  if (modifiedTouched.length) {
    parts.push(
      `changed high-risk UI is still skip-only — add *.browser.test.tsx or stack-browser evidence before merging:\n${formatTouchedList(modifiedTouched)}`,
    );
  }
  errors.push(parts.join("\n"));
}

// New product .tsrx under components/ that is already high-risk but missing from
// manifest is covered above. Also fail when a newly added high-risk path was
// listed in the diff but somehow not discovered (shouldn't happen).
if (uiDiff) {
  const highRiskAddedMissing = uiDiff.added.filter(
    (rel) =>
      !EXCLUDE_RE.test(rel) &&
      existsSync(join(srcDir, rel)) &&
      HIGH_RISK_RE.test(readFileSync(join(srcDir, rel), "utf8")) &&
      !bySurface.has(rel),
  );
  if (highRiskAddedMissing.length) {
    errors.push(
      `newly added high-risk .tsrx missing from browserCoverageManifest:\n${formatTouchedList(highRiskAddedMissing)}`,
    );
  }
}

const mode = uiDiff
  ? `change-aware base=${uiDiff.base} touched=${uiDiff.touched.length}`
  : "inventory-only";
console.log(
  `browser-coverage-check: mode=${mode} discovered=${discovered.length} browser=${withBrowser} stack-browser=${withStack} skip-only=${withSkipOnly}`,
);

if (errors.length) {
  for (const e of errors) console.error(`browser-coverage-check: FAIL: ${e}`);
  process.exit(1);
}

console.log("browser-coverage-check: PASS");
