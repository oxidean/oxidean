/**
 * `bump` — apply a computed plan (WF-3, BR3.7, FR3.3). Writes only the
 * in-registry version carriers, then regenerates Cargo.lock and bun.lock via
 * the native toolchains. A failed write or regen exits 1 — partial bumps are
 * reported, never claimed clean.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { RegistryEntry } from "../lib/registry.ts";
import type { BumpPlan } from "../lib/plan.ts";
import { carrierVersion } from "../lib/plan.ts";
import { compareSemver } from "../lib/semver.ts";
import { buildPlan, printPlanTable } from "./plan.ts";

export const usage =
  "bump [--since <ref>] [--channel <c>] [--plan <file>] [--dry-run]\n" +
  "                                apply the plan: version writes + lockfile regen";

export interface ManifestEdit {
  path: string;
  componentId: string;
  field:
    | "cargo.version"
    | "package.version"
    | "wire_constant"
    | "chart.version"
    | "chart.app_version"
    | "chart.image_tag";
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

/** Rewrite the top-level `version:` scalar of a Helm Chart.yaml. */
function writeChartVersion(text: string, after: string): string | null {
  if (!/^version:[ \t]*"?[^"\s]+"?[ \t]*$/m.test(text)) return null;
  return text.replace(/^version:[ \t]*"?[^"\s]+"?[ \t]*$/m, `version: "${after}"`);
}

/** Rewrite the top-level `appVersion:` scalar of a Helm Chart.yaml. */
function writeChartAppVersion(text: string, after: string): string | null {
  if (!/^appVersion:[ \t]*"?[^"\s]+"?[ \t]*$/m.test(text)) return null;
  return text.replace(/^appVersion:[ \t]*"?[^"\s]+"?[ \t]*$/m, `appVersion: "${after}"`);
}

/** Rewrite `<svc>: { image: { tag: } }` in a Helm values.yaml (line-scoped). */
function writeValuesImageTag(text: string, svc: string, after: string): string | null {
  const lines = text.split("\n");
  let inSvc = false;
  let inImage = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const t = line.trim();
    if (/^[a-z]/.test(line)) {
      inSvc = t === `${svc}:`;
      inImage = false;
      continue;
    }
    if (!inSvc) continue;
    if (/^  \S/.test(line) && !/^   /.test(line)) {
      inImage = t === "image:";
      continue;
    }
    if (inImage && /^    tag:/.test(line)) {
      lines[i] = line.replace(/tag:[ \t]*.*$/, `tag: "${after}"`);
      return lines.join("\n");
    }
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
      : entry.manifest_path.endsWith("Chart.yaml")
        ? "chart.version"
        : "package.version";
    let text: string;
    try {
      text = readFileSync(abs, "utf-8");
    } catch {
      return { error: `${entry.id}: cannot read ${entry.manifest_path}` };
    }
    const next = field === "cargo.version"
      ? writeCargoVersion(text, after)
      : field === "chart.version"
        ? writeChartVersion(text, after)
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

/**
 * Helm post-pass: after all carriers are written, stamp a kind=helm entry's
 * `appVersion:` from its app_component's (possibly just-bumped) carrier and
 * `values.yaml` `<svc>.image.tag` pins from image_components. Stamps are
 * idempotent — re-running a bump that changed no source carrier is a no-op.
 */
export function applyHelmExtras(
  rootDir: string,
  entry: RegistryEntry,
  byId: Map<string, RegistryEntry>,
  bumped: ReadonlyMap<string, string> = new Map(),
): (ManifestEdit | { error: string })[] {
  const out: (ManifestEdit | { error: string })[] = [];
  if (!entry.manifest_path) return out;
  const chartRel = entry.manifest_path;
  const chartAbs = join(rootDir, chartRel);
  let chart: string;
  try {
    chart = readFileSync(chartAbs, "utf-8");
  } catch {
    return [{ error: `${entry.id}: cannot read ${chartRel}` }];
  }

  const resolve = (id: string): string | null => {
    const target = byId.get(id);
    return target ? carrierVersion(rootDir, target) : null;
  };

  if (entry.app_component) {
    const v = resolve(entry.app_component);
    if (!v) {
      out.push({ error: `${entry.id}: cannot resolve app_component "${entry.app_component}" carrier` });
    } else {
      const prev = chart.match(/^appVersion:[ \t]*"?([^"\s]+)"?[ \t]*$/m)?.[1] ?? "";
      const next = writeChartAppVersion(chart, v);
      if (next === null) {
        out.push({ error: `${entry.id}: no appVersion field in ${chartRel}` });
      } else if (prev !== v) {
        chart = next;
        out.push({ path: chartRel, componentId: entry.id, field: "chart.app_version", before: prev, after: v });
      }
    }
  }

  const images = entry.image_components ?? {};
  if (Object.keys(images).length > 0) {
    const valuesRel = join(dirname(chartRel), "values.yaml");
    const valuesAbs = join(rootDir, valuesRel);
    let values: string | null = null;
    try {
      values = readFileSync(valuesAbs, "utf-8");
    } catch {
      out.push({ error: `${entry.id}: cannot read ${valuesRel}` });
    }
    if (values !== null) {
      let changed = false;
      for (const [svc, comp] of Object.entries(images)) {
        const cids = Array.isArray(comp) ? comp : [comp];
        const prevTag = values.match(new RegExp(`^${svc}:[\\s\\S]*?^    tag:[ \\t]*"?([^"\\s]*)"?`, "m"))?.[1];
        // A shared image published by several components (web ↔ oxidean-web)
        // must pin the tag this release actually produced — the bumped
        // carrier's new version — not the numerically greatest series
        // version, which can name an image built from older code. With no
        // carrier in this plan, the existing pin still names a published
        // tag; only fill it when empty.
        const released = cids.filter((c) => bumped.has(c));
        let v: string | null = null;
        if (released.length > 0) {
          for (const cid of released) {
            const cv = bumped.get(cid)!;
            if (v === null || compareSemver(cv, v) > 0) v = cv;
          }
        } else if (!prevTag) {
          for (const cid of cids) {
            const cv = resolve(cid);
            if (cv !== null && (v === null || compareSemver(cv, v) > 0)) v = cv;
          }
        } else {
          continue;
        }
        if (!v) {
          out.push({ error: `${entry.id}: cannot resolve image_components.${svc} = "${JSON.stringify(comp)}" carrier` });
          continue;
        }
        const next = writeValuesImageTag(values, svc, v);
        if (next === null) {
          out.push({ error: `${entry.id}: no ${svc}.image.tag field in ${valuesRel}` });
          continue;
        }
        if (prevTag !== v) {
          values = next;
          changed = true;
          out.push({
            path: valuesRel,
            componentId: entry.id,
            field: "chart.image_tag",
            before: prevTag ?? "",
            after: `${svc}=${v}`,
          });
        }
      }
      if (changed) writeFileSync(valuesAbs, values);
    }
  }

  writeFileSync(chartAbs, chart);
  return out;
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
  // Helm stamps run after every carrier write so appVersion/image tags read
  // the freshly bumped versions, not the pre-bump ones.
  const bumped = new Map(plan.entries.map((e) => [e.component, e.toVersion]));
  for (const e of plan.entries) {
    const reg = byId.get(e.component);
    if (!reg || reg.kind !== "helm") continue;
    for (const x of applyHelmExtras(root, reg, byId, bumped)) {
      if ("error" in x) errors.push(x.error);
      else edits.push(x);
    }
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
