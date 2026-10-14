/**
 * `plan` — compute the release plan (WF-2, contract C3 output).
 * Default prints a human table; `--json` emits the release-plan payload that
 * `changelog`/`tag` (U4) and the release-PR workflow consume.
 */
import { readFileSync } from "node:fs";
import { liveGit } from "../lib/git.ts";
import { collectRange, computePlan, type BumpPlan } from "../lib/plan.ts";
import type { RegistryConfig } from "../lib/registry.ts";
import { loadOrFail, parseCommonArgs } from "./detect.ts";

export const usage =
  "plan [--since <ref>] [--channel stable|alpha|beta|nightly] [--json]\n" +
  "                                compute the release plan (C3)";

export function buildPlan(root: string, args: string[]):
  | { ok: true; plan: BumpPlan; registry: RegistryConfig; json: boolean; dryRun: boolean }
  | { ok: false; code: number } {
  const flags = parseCommonArgs(args);
  if (flags.error) {
    process.stderr.write(`plan: ${flags.error}\n`);
    return { ok: false, code: 2 };
  }
  const registry = loadOrFail(root);
  if (registry === null) return { ok: false, code: 1 };

  // --plan <file>: one computed plan drives changelog/bump/release-tags so a
  // cut never drifts — recomputing after `bump` would read the already-bumped
  // carriers as fromVersion and double-bump (0.1.1 -> 0.1.2).
  if (flags.plan !== undefined) {
    try {
      const raw = JSON.parse(readFileSync(flags.plan, "utf-8")) as BumpPlan;
      if (raw.schema !== "release-plan" || raw.version !== 1 || !Array.isArray(raw.entries)) {
        throw new Error("not a release-plan v1 payload");
      }
      // C3 output strips advisory fields — restore defaults so consumers see
      // a complete BumpPlan regardless of how the file was produced.
      const plan: BumpPlan = {
        ...raw,
        channel: raw.channel ?? "stable",
        warnings: raw.warnings ?? [],
      };
      return { ok: true, plan, registry, json: flags.json, dryRun: flags.dryRun };
    } catch (e) {
      process.stderr.write(`plan: cannot load ${flags.plan}: ${e instanceof Error ? e.message : String(e)}\n`);
      return { ok: false, code: 2 };
    }
  }

  const git = liveGit(root);
  try {
    const range = collectRange(root, git, registry, flags.since, flags.channel);
    const plan = computePlan(registry, range.commits, {
      rootDir: root,
      git,
      channel: flags.channel,
      componentBounds: range.componentBounds,
    });
    return { ok: true, plan, registry, json: flags.json, dryRun: flags.dryRun };
  } catch (e) {
    process.stderr.write(`plan: ${e instanceof Error ? e.message : String(e)}\n`);
    return { ok: false, code: 2 };
  }
}

export function printPlanTable(plan: BumpPlan): void {
  if (plan.entries.length === 0) {
    process.stdout.write("plan: no bumps — range produced no releasable changes\n");
    return;
  }
  process.stdout.write(
    `${"component".padEnd(16)} ${"from".padEnd(10)} ${"to".padEnd(22)} ${"bump".padEnd(6)} evidence\n`,
  );
  for (const e of plan.entries) {
    const ev = [
      e.direct ? `${e.commits.length} commit(s)` : "",
      e.propagatedFrom.length > 0 ? `via ${e.propagatedFrom.join(", ")}` : "",
    ]
      .filter(Boolean)
      .join(" + ");
    process.stdout.write(
      `${e.component.padEnd(16)} ${e.fromVersion.padEnd(10)} ${e.toVersion.padEnd(22)} ${e.bump.padEnd(6)} ${ev}\n`,
    );
  }
}

export function planCommand(root: string, args: string[]): number {
  const res = buildPlan(root, args);
  if (!res.ok) return res.code;
  const { plan } = res;
  if (res.json) {
    // C3 payload is exactly {schema, version, generatedAt, baseRef, entries}.
    const { schema, version, generatedAt, baseRef, entries } = plan;
    process.stdout.write(
      JSON.stringify({ schema, version, generatedAt, baseRef, entries }, null, 2) + "\n",
    );
  } else {
    printPlanTable(plan);
  }
  for (const w of plan.warnings) process.stderr.write(`warn: ${w}\n`);
  return 0;
}
