#!/usr/bin/env bun
/**
 * Issue #162 — guard the api domain-crate split. Asserts:
 *   1. Dependency direction: domain crates may only depend on
 *      `oxidean-api-core` among `oxidean-api*` crates (never on one another);
 *      `oxidean-api-core` depends on none of them; the shell `oxidean-api`
 *      may compose all of them.
 *   2. Procedure ownership is disjoint: every `PROCEDURES` const across the
 *      domain crates plus core's residual list contains each procedure name
 *      at most once — a duplicated name would shadow silently in dispatch.
 *
 * Bun port of the original .sh+python3 heredoc — the ci:tools image has no
 * python3, so this gate must run on bun like the other sync checks.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..");
const fail = [];

const apiDep = /^(oxidean-api[a-z-]*)\s*=/gm;
const section = /^\[(?:build-|dev-)?dependencies\](.*?)(?=^\[|$)/gms;
const pkgName = /^name\s*=\s*"([^"]+)"/m;
const procBlock = /pub const PROCEDURES\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\];/gs;
const procName = /^\s*"([a-zA-Z][a-zA-Z0-9._]*)",\s*$/gm;

const manifests = readdirSync(join(ROOT, "crates"))
  .filter((d) => d.startsWith("oxidean-api"))
  .map((d) => join(ROOT, "crates", d, "Cargo.toml"));

for (const manifest of manifests) {
  const data = readFileSync(manifest, "utf8");
  const name = data.match(pkgName)?.[1];
  const deps = new Set();
  for (const s of data.matchAll(section)) {
    for (const d of s[1].matchAll(apiDep)) deps.add(d[1]);
  }
  if (name === "oxidean-api") continue; // the composition shell may use all
  const allowed = name === "oxidean-api-core" ? new Set() : new Set(["oxidean-api-core"]);
  const bad = [...deps].filter((d) => !allowed.has(d));
  if (bad.length) {
    fail.push(
      `${name} depends on ${bad.sort()} — domain crates may only use ` +
        `${allowed.size ? [...allowed] : "no api-* crate"}`,
    );
  }
}

const seen = new Map();
const sources = readdirSync(join(ROOT, "crates"))
  .filter((d) => d.startsWith("oxidean-api-"))
  .map((d) => join(ROOT, "crates", d, "src", "lib.rs"));
sources.push(join(ROOT, "crates", "oxidean-api-core", "src", "rpc.rs"));

let scanned = 0;
for (const srcPath of sources) {
  const owner = srcPath.split("/").at(-3);
  let data;
  try {
    data = readFileSync(srcPath, "utf8");
  } catch {
    continue;
  }
  for (const b of data.matchAll(procBlock)) {
    scanned++;
    for (const p of b[1].matchAll(procName)) {
      if (seen.has(p[1])) {
        fail.push(`procedure ${p[1]} is owned by both ${seen.get(p[1])} and ${owner}`);
      } else {
        seen.set(p[1], owner);
      }
    }
  }
}

console.log(`api-domain-graph: ${seen.size} procedures across ${scanned} PROCEDURES lists`);
if (fail.length) {
  for (const line of fail) console.error(`error: ${line}`);
  process.exit(1);
}
console.log("api-domain-graph: ok");
