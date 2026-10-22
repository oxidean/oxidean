#!/usr/bin/env bun
/**
 * Refresh stack-presets from declared catalog sources (issue #18).
 *
 * Usage:
 *   bun scripts/sync-stack-presets.mjs           # print plan + update last_synced for manual packs
 *   bun scripts/sync-stack-presets.mjs --pack nextjs
 *   bun scripts/sync-stack-presets.mjs --dry-run
 *
 * Full CLI regeneration (create-next-app, cargo new, …) is documented per pack in
 * catalog.json `source_ref`. This driver updates metadata and can invoke generators
 * when the toolchain is available; otherwise maintainers commit official-style trees
 * produced offline and re-run with --stamp.
 */

import { readFileSync, writeFileSync, existsSync, readdirSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const PRESETS = join(ROOT, "crates/oxidean-api-repo/assets/stack-presets");
const CATALOG_PATH = join(PRESETS, "catalog.json");

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const stampOnly = args.includes("--stamp");
const packIdx = args.indexOf("--pack");
const onlyPack = packIdx >= 0 ? args[packIdx + 1] : null;

const catalog = JSON.parse(readFileSync(CATALOG_PATH, "utf8"));
const today = new Date().toISOString().slice(0, 10);

let changed = false;
for (const pack of catalog.packs) {
  if (onlyPack && pack.id !== onlyPack) continue;
  const dir = join(PRESETS, pack.id);
  if (!existsSync(dir)) {
    console.error(`missing pack dir: ${pack.id}`);
    process.exit(1);
  }
  console.log(
    `${pack.id}\tsource=${pack.source}\tref=${pack.source_ref}\tfiles=${countFiles(dir)}`,
  );
  if (stampOnly || pack.source === "manual" || pack.source === "oxidean") {
    if (pack.last_synced !== today) {
      pack.last_synced = today;
      changed = true;
    }
  }
}

if (changed && !dryRun) {
  writeFileSync(CATALOG_PATH, JSON.stringify(catalog, null, 2) + "\n");
  console.log("updated catalog.json last_synced");
} else if (dryRun) {
  console.log("dry-run: no writes");
} else {
  console.log(
    "No catalog metadata changes. To regenerate trees, run the CLI in source_ref into a temp dir, strip node_modules/.git, copy into the pack, then re-run with --stamp.",
  );
}

function countFiles(dir) {
  let n = 0;
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, ent.name);
    if (ent.isDirectory()) n += countFiles(p);
    else if (ent.isFile()) n += 1;
  }
  return n;
}
