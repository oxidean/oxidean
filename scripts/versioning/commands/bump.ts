/**
 * `bump` — apply a computed plan (WF-3, BR3.7, FR3.3). Writes only the
 * in-registry version carriers, then regenerates Cargo.lock and bun.lock via
 * the native toolchains. A failed write or regen exits 1 — partial bumps are
 * reported, never claimed clean.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { RegistryEntry } from "../lib/registry.ts";
import type { BumpPlan } from "../lib/plan.ts";
import { carrierVersion } from "../lib/plan.ts";
import { buildPlan, printPlanTable } from "./plan.ts";

export const usage =
  "bump [--since <ref>] [--channel <c>] [--plan <file>] [--dry-run]\n" +
  "                                apply the plan: version writes + lockfile regen";

export interface ManifestEdit {
  path: string;
  componentId: string;
  field: "cargo.version" | "package.version" | "wire_constant";
  before: string;
  after: string;
}

/** Rewrite the `[package] version` line of a Cargo.toml. */
function writeCargoVersion(text: string, after: string): string | null {
  const lines = text.split("\n");
  let inPackage = false;
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    if (/^\[/.test(t)) inPackage = t === "[package]";
    else if (inPackage && /^version\s*=/.test(t)) {
      lines[i] = lines[i].replace(/version\s*=\s*"[^"]*"/, `version = "${after}"`);
      return lines.join("\n");
    }
  }
  return null;
}

/** Rewrite the top-level `"version": "x"` of a package.json. */
function writePackageVersion(text: string, after: string): string | null {
  try {
    const pkg = JSON.parse(text) as { version?: unknown };
    if (typeof pkg.version !== "string") return null;
    // Replace only the first `"version":` occurrence — requiring it to hold
    // the exact current literal keeps nested/tool "version" keys safe.
    const re = /"version"\s*:\s*"([^"]*)"/;
    const m = text.match(re);
    if (!m || m[1] !== pkg.version) return null;
    return text.replace(re, `"version": "${after}"`);
  } catch {
    return null;
  }
}

/** Rewrite a `file#NAME` constant's literal (`pub const NAME: &str = "…"` or `const NAME = "…"`). */
function writeConstant(text: string, name: string, after: string): string | null {
  const res = [
    new RegExp(`(pub\\s+const\\s+${name}\\s*:\\s*&str\\s*=\\s*")[^"]*(")`),
    new RegExp(`((?:export\\s+)?const\\s+${name}\\s*=\\s*")[^"]*(")`),
  ];
  for (const re of res) {
    if (re.test(text)) return text.replace(re, `$1${after}$2`);
  }
  return null;
}

/** Apply one plan entry's version write. Returns the edit or an error. */
export function applyEntry(
  rootDir: string,
  entry: RegistryEntry,
  before: string,
  after: string,
): ManifestEdit | { error: string } {
  if (entry.manifest_path) {
    const abs = join(rootDir, entry.manifest_path);
    const field = entry.manifest_path.endsWith("Cargo.toml")
      ? "cargo.version"
      : "package.version";
    let text: string;
    try {
      text = readFileSync(abs, "utf-8");
    } catch {
      return { error: `${entry.id}: cannot read ${entry.manifest_path}` };
    }
    const next = field === "cargo.version"
      ? writeCargoVersion(text, after)
      : writePackageVersion(text, after);
    if (next === null) {
      return { error: `${entry.id}: no version field found in ${entry.manifest_path}` };
    }
    writeFileSync(abs, next);
    return { path: entry.manifest_path, componentId: entry.id, field, before, after };
  }
  const cp = entry.constant_path!;
  const hash = cp.lastIndexOf("#");
  const file = cp.slice(0, hash);
  const name = cp.slice(hash + 1);
  const abs = join(rootDir, file);
  let text: string;
  try {
    text = readFileSync(abs, "utf-8");
  } catch {
    return { error: `${entry.id}: cannot read ${file}` };
  }
  const next = writeConstant(text, name, after);
  if (next === null) return { error: `${entry.id}: constant ${name} not found in ${file}` };
  writeFileSync(abs, next);
  return { path: cp, componentId: entry.id, field: "wire_constant", before, after };
}

function regen(rootDir: string, cmd: string[]): string | null {
  const proc = Bun.spawnSync(cmd, { cwd: rootDir, stdout: "pipe", stderr: "pipe" });
  return proc.exitCode === 0 ? null : `${cmd.join(" ")} failed: ${proc.stderr.toString().slice(0, 300)}`;
}

/** Regenerate lockfiles unconditionally after manifest writes (BR3.7). */
export function regenLockfiles(rootDir: string): string[] {
  const errors: string[] = [];
  for (const cmd of [
    ["cargo", "metadata", "--format-version", "1", "--quiet"],
    ["bun", "install", "--lockfile-only"],
  ]) {
    const err = regen(rootDir, cmd);
    if (err) errors.push(err);
  }
  return errors;
}

export function bumpCommand(root: string, args: string[]): number {
  const res = buildPlan(root, args);
  if (!res.ok) return res.code;
  const { plan } = res;
  for (const w of plan.warnings) process.stderr.write(`warn: ${w}\n`);

  if (res.dryRun || plan.entries.length === 0) {
    printPlanTable(plan);
    if (res.dryRun) process.stdout.write("bump: dry-run — nothing written\n");
    return 0;
  }

  const byId = new Map(res.registry.entries.map((e) => [e.id, e]));
  const edits: ManifestEdit[] = [];
  const errors: string[] = [];
  // Stale-plan guard: with --plan the plan predates this process; refuse to
  // write when a carrier no longer reads the plan's fromVersion — otherwise a
  // post-bump recompute would silently apply a second bump.
  if (args.includes("--plan")) {
    for (const e of plan.entries) {
      const reg = byId.get(e.component);
      if (!reg) continue;
      const current = carrierVersion(root, reg);
      if (current !== e.fromVersion) {
        errors.push(
          `${e.component}: carrier reads ${current}, plan expects ${e.fromVersion} — stale plan`,
        );
      }
    }
    if (errors.length > 0) {
      for (const er of errors) process.stderr.write(`bump: ${er}\n`);
      return 1;
    }
  }
  for (const e of plan.entries) {
    const reg = byId.get(e.component);
    if (!reg) {
      errors.push(`${e.component}: not in registry`);
      continue;
    }
    const applied = applyEntry(root, reg, e.fromVersion, e.toVersion);
    if ("error" in applied) errors.push(applied.error);
    else edits.push(applied);
  }
  if (edits.length > 0) errors.push(...regenLockfiles(root));

  for (const ed of edits) {
    process.stdout.write(`${ed.path}: ${ed.componentId} ${ed.before} -> ${ed.after} (${ed.field})\n`);
  }
  if (errors.length > 0) {
    for (const er of errors) process.stderr.write(`bump: ${er}\n`);
    return 1;
  }
  return 0;
}
