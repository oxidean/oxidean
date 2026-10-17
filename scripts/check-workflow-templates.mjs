#!/usr/bin/env bun
// Validate vendored workflow templates: yml ↔ properties.json pairing, required
// properties fields, referenced icons exist, no stray files, size budget.
// Node/Bun only — the ci-tools image intentionally ships no python3.
import { readdirSync, readFileSync, statSync, existsSync, statSync as stat } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const TEMPLATES = join(ROOT, "crates/oxidean-api/assets/workflow-templates");
const CATALOG = join(TEMPLATES, "catalog.json");
const MAX_TOTAL_BYTES = 1024 * 1024;

const err = (m) => console.error(`error: ${m}`);
const warn = (m) => console.error(`warning: ${m}`);

if (!existsSync(CATALOG)) {
  err(`missing ${CATALOG}`);
  process.exit(1);
}
const catalog = JSON.parse(readFileSync(CATALOG, "utf8"));
const dirs = catalog.include_dirs;
const allowedTop = new Set([...dirs, "icons", "catalog.json"]);
let fail = 0;

for (const key of ["repo", "commit", "synced"]) {
  if (!(key in catalog.upstream)) {
    err(`catalog upstream missing ${key}`);
    fail = 1;
  }
}
if (!("excluded" in catalog)) {
  err("catalog missing excluded map");
  fail = 1;
}

const isDir = (p) => existsSync(p) && statSync(p).isDirectory();
const isFile = (p) => existsSync(p) && statSync(p).isFile();

let templates = 0;
for (const d of dirs) {
  const ddir = join(TEMPLATES, d);
  if (!isDir(ddir)) {
    err(`missing dir ${ddir}`);
    fail = 1;
    continue;
  }
  for (const ent of readdirSync(ddir)) {
    const p = join(ddir, ent);
    if (isDir(p)) {
      err(`unexpected subdir ${d}/${ent}`);
      fail = 1;
      continue;
    }
    if (ent.endsWith(".yml") || ent.endsWith(".yaml")) {
      const stem = ent.slice(0, ent.lastIndexOf("."));
      const props = join(ddir, `${stem}.properties.json`);
      if (!isFile(props)) {
        err(`${d}/${ent} has no properties.json`);
        fail = 1;
        continue;
      }
      const propsData = JSON.parse(readFileSync(props, "utf8"));
      for (const key of ["name", "description", "iconName", "categories"]) {
        if (!(key in propsData)) {
          err(`${d}/${stem}.properties.json missing ${key}`);
          fail = 1;
        }
      }
      const icon = propsData.iconName;
      if (icon && !isFile(join(TEMPLATES, "icons", `${icon}.svg`))) {
        warn(`${d}/${stem} icon '${icon}' not vendored (octicon refs are expected)`);
      }
      templates += 1;
    } else if (ent.endsWith(".properties.json")) {
      const stem = ent.slice(0, -".properties.json".length);
      if (!isFile(join(ddir, `${stem}.yml`)) && !isFile(join(ddir, `${stem}.yaml`))) {
        err(`${d}/${ent} has no workflow file`);
        fail = 1;
      }
    } else {
      err(`unexpected file ${d}/${ent}`);
      fail = 1;
    }
  }
}

for (const ent of readdirSync(TEMPLATES)) {
  if (!allowedTop.has(ent)) {
    err(`unexpected top-level entry ${ent}`);
    fail = 1;
  }
}

for (const key of Object.keys(catalog.templates ?? {})) {
  const [d, stem] = key.split("/", 2);
  if (
    !dirs.includes(d) ||
    (!isFile(join(TEMPLATES, d, `${stem}.yml`)) && !isFile(join(TEMPLATES, d, `${stem}.yaml`)))
  ) {
    err(`catalog templates entry '${key}' has no vendored template`);
    fail = 1;
  }
}

// compat ledger: every vendored workflow has a classified entry; values are
// recomputed and verified by tests/actions_catalog_ledger.rs.
const COMPAT_STATUSES = new Set([
  "runnable",
  "partial",
  "unsupported-runtime",
  "reusable-workflow",
  "restricted",
  "parse_error",
]);
const compat = catalog.compat ?? {};
const compatKeys = new Set(Object.keys(compat));
for (const d of dirs) {
  for (const ent of readdirSync(join(TEMPLATES, d))) {
    if (!ent.endsWith(".yml") && !ent.endsWith(".yaml")) continue;
    const key = `${d}/${ent.slice(0, ent.lastIndexOf("."))}`;
    const entry = compat[key];
    if (!entry) {
      err(`catalog compat missing '${key}'`);
      fail = 1;
      continue;
    }
    if (!COMPAT_STATUSES.has(entry.status)) {
      err(`catalog compat '${key}' has unknown status '${entry.status}'`);
      fail = 1;
    }
    for (const f of ["missing_features", "unsupported_actions", "warnings"]) {
      if (!Array.isArray(entry[f])) {
        err(`catalog compat '${key}.${f}' must be an array`);
        fail = 1;
      }
    }
  }
}
for (const key of compatKeys) {
  const [d, stem] = key.split("/", 2);
  if (
    !dirs.includes(d) ||
    (!isFile(join(TEMPLATES, d, `${stem}.yml`)) && !isFile(join(TEMPLATES, d, `${stem}.yaml`)))
  ) {
    err(`catalog compat entry '${key}' has no vendored template`);
    fail = 1;
  }
}

console.error(`check: ${templates} templates validated`);

// Total vendored size budget.
let total = 0;
const walk = (dir) => {
  for (const ent of readdirSync(dir)) {
    const p = join(dir, ent);
    const s = stat(p);
    if (s.isDirectory()) walk(p);
    else total += s.size;
  }
};
walk(TEMPLATES);
if (total > MAX_TOTAL_BYTES) {
  err(`workflow-templates ${total} bytes exceeds ${MAX_TOTAL_BYTES}`);
  fail = 1;
}

if (fail !== 0) process.exit(1);
console.log(`check-workflow-templates: ok (${total} bytes)`);
