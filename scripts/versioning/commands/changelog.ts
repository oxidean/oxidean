/**
 * `changelog` — render plan entries into Keep-a-Changelog files (WF-1, FR4.x).
 * Reuses `buildPlan` (never a divergent plan); per-component sections upsert
 * idempotently into `changelog_path`; the root CHANGELOG.md consolidates.
 */
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import {
  renderRootSection,
  renderSection,
  upsertSection,
} from "../lib/changelog.ts";
import { buildPlan } from "./plan.ts";

export const usage =
  "changelog [--since <ref>] [--channel <c>] [--dry-run]\n" +
  "                                write Keep-a-Changelog sections for the plan";

const ROOT_CHANGELOG = "CHANGELOG.md";
const ROOT_HEADER =
  "# Changelog\n\nConsolidated release log — each section links every bumped\n" +
  "component's tag and its component changelog.\n";

function today(): string {
  return new Date().toISOString().slice(0, 10);
}

export function changelogCommand(root: string, args: string[]): number {
  const built = buildPlan(root, args);
  if (!built.ok) return built.code;
  const { plan, registry, dryRun } = built;
  const date = today();
  const touched = plan.entries.filter((e) => e.fromVersion !== e.toVersion);
  const written: string[] = [];

  for (const entry of touched) {
    const reg = registry.entries.find((c) => c.id === entry.component);
    if (!reg?.changelog_path) continue;
    const path = join(root, reg.changelog_path);
    const before = existsSync(path) ? readFileSync(path, "utf-8") : "";
    const after = upsertSection(before, entry.toVersion, renderSection(entry, date));
    if (!dryRun) {
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, after);
    }
    written.push(reg.changelog_path);
  }

  // Root consolidation — one section per release cut (FR4.1).
  if (touched.length > 0) {
    const path = join(root, ROOT_CHANGELOG);
    const before = existsSync(path) ? readFileSync(path, "utf-8") : ROOT_HEADER;
    const lines = before.split("\n");
    const first = lines.findIndex((l) => /^## /.test(l));
    const section = renderRootSection(plan.entries, date).trimEnd() + "\n";
    const after =
      first === -1
        ? before.trimEnd() + "\n\n" + section
        : [...lines.slice(0, first), section, ...lines.slice(first)].join("\n");
    if (!dryRun) writeFileSync(path, after);
    written.push(ROOT_CHANGELOG);
  }

  if (touched.length === 0) {
    process.stdout.write("changelog: plan is empty — nothing to write\n");
    return 0;
  }
  process.stdout.write(
    `${dryRun ? "changelog (dry-run)" : "changelog"}: wrote ${written.length} file(s):\n` +
      written.map((w) => `  ${w}`).join("\n") +
      "\n",
  );
  return 0;
}
