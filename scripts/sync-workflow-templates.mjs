#!/usr/bin/env bun
/**
 * Vendor GitHub starter-workflows into crates/oxidean-api/assets/workflow-templates.
 *
 * Usage:
 *   bun scripts/sync-workflow-templates.mjs                  # sync at the pinned commit
 *   bun scripts/sync-workflow-templates.mjs --commit <sha>   # re-pin + sync
 *   bun scripts/sync-workflow-templates.mjs --check          # byte-for-byte drift check
 *
 * Upstream layout: <dir>/<name>.yml with metadata in <dir>/properties/<name>.properties.json.
 * Vendored layout flattens properties next to the workflow so every <name>.yml has a
 * sibling <name>.properties.json. Only icons referenced by vendored properties are kept.
 * catalog.json `templates` is a hand-maintained metadata overlay (e.g. file_patterns)
 * and survives syncs; `upstream` pin and `synced` date are rewritten.
 */

import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
  cpSync,
  mkdirSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const DEST = join(ROOT, "crates/oxidean-api/assets/workflow-templates");
const CATALOG_PATH = join(DEST, "catalog.json");

const REPO = "actions/starter-workflows";
const INCLUDE_DIRS = ["ci", "deployments", "automation"];
const EXCLUDED = {
  "code-scanning": "requires GitHub Advanced Security / CodeQL",
  pages: "GitHub Pages-specific publishing",
};

const args = process.argv.slice(2);
const checkOnly = args.includes("--check");
const commitIdx = args.indexOf("--commit");
const catalog = existsSync(CATALOG_PATH)
  ? JSON.parse(readFileSync(CATALOG_PATH, "utf8"))
  : null;
const pinned = commitIdx >= 0 ? args[commitIdx + 1] : catalog?.upstream?.commit;
if (!pinned) {
  console.error("no pinned commit in catalog.json; pass --commit <sha>");
  process.exit(1);
}
if (checkOnly && commitIdx >= 0) {
  console.error("--check does not accept --commit");
  process.exit(1);
}

const tmp = mkdtempSync(join(tmpdir(), "workflow-templates-"));
try {
  const tar = join(tmp, "src.tar.gz");
  execFileSync(
    "curl",
    ["-fsSL", "-o", tar, `https://codeload.github.com/${REPO}/tar.gz/${pinned}`],
    { stdio: "inherit" },
  );
  execFileSync("tar", ["-xzf", tar, "-C", tmp]);
  const src = join(tmp, `starter-workflows-${pinned}`);
  if (!existsSync(src)) {
    console.error(`unexpected tarball layout: no starter-workflows-${pinned}`);
    process.exit(1);
  }

  const work = join(tmp, "vendor");
  mkdirSync(work, { recursive: true });
  syncInto(src, work);

  if (checkOnly) {
    // Same layout but under a scratch root; compare against the real tree.
    const diff = diffTrees(DEST, work);
    if (diff.length) {
      for (const d of diff) console.error(`drift: ${d}`);
      process.exit(1);
    }
    console.log("check-workflow-templates: no drift at", pinned);
    process.exit(0);
  }

  rmSync(DEST, { recursive: true, force: true });
  cpSync(work, DEST, { recursive: true });

  const today = new Date().toISOString().slice(0, 10);
  const next = {
    upstream: { repo: REPO, commit: pinned, synced: today },
    include_dirs: INCLUDE_DIRS,
    excluded: EXCLUDED,
    templates: catalog?.templates ?? {},
  };
  writeFileSync(CATALOG_PATH, JSON.stringify(next, null, 2) + "\n");
  console.log(`synced ${REPO}@${pinned.slice(0, 12)} → assets/workflow-templates`);
} finally {
  rmSync(tmp, { recursive: true, force: true });
}

function syncInto(src, dest) {
  const wantedIcons = new Set();
  for (const dir of INCLUDE_DIRS) {
    const srcDir = join(src, dir);
    const outDir = join(dest, dir);
    mkdirSync(outDir, { recursive: true });
    for (const ent of readdirSync(srcDir)) {
      if (!/\.ya?ml$/.test(ent)) continue;
      const stem = ent.replace(/\.ya?ml$/, "");
      cpSync(join(srcDir, ent), join(outDir, `${stem}.yml`));
      const props = join(srcDir, "properties", `${stem}.properties.json`);
      if (existsSync(props)) {
        cpSync(props, join(outDir, `${stem}.properties.json`));
        const icon = JSON.parse(readFileSync(props, "utf8")).iconName;
        if (icon && existsSync(join(src, "icons", `${icon}.svg`))) {
          wantedIcons.add(icon);
        }
      } else {
        console.warn(`warning: ${dir}/${stem}.yml has no properties.json`);
      }
    }
  }
  const iconDest = join(dest, "icons");
  mkdirSync(iconDest, { recursive: true });
  for (const icon of wantedIcons) {
    cpSync(join(src, "icons", `${icon}.svg`), join(iconDest, `${icon}.svg`));
  }
  console.log(
    `vendored ${INCLUDE_DIRS.map((d) => `${d}:${readdirSync(join(dest, d)).filter((f) => f.endsWith(".yml")).length}`).join(" ")} icons:${wantedIcons.size}`,
  );
}

function diffTrees(a, b) {
  const out = [];
  const filesA = listFiles(a, "");
  const filesB = listFiles(b, "");
  for (const f of filesA) {
    if (f === "catalog.json") continue; // rewritten each sync; checked separately
    if (!filesB.includes(f)) out.push(`${f} missing upstream`);
    else if (readFileSync(join(a, f), "utf8") !== readFileSync(join(b, f), "utf8"))
      out.push(`${f} differs`);
  }
  for (const f of filesB) {
    if (!filesA.includes(f)) out.push(`${f} not vendored`);
  }
  return out;
}

function listFiles(root, rel) {
  const out = [];
  for (const ent of readdirSync(join(root, rel), { withFileTypes: true })) {
    const r = rel ? `${rel}/${ent.name}` : ent.name;
    if (ent.isDirectory()) out.push(...listFiles(root, r));
    else out.push(r);
  }
  return out;
}
