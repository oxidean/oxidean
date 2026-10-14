/**
 * Keep-a-Changelog section render + idempotent upsert (U4, FR4.x).
 *
 * Sections are keyed on `## [<version>] - <date>`; regenerating a version
 * replaces the section in place — never appends a duplicate (FR4.2).
 */
import type { PlanEntry } from "./plan.ts";

const BUCKETS: Record<string, string> = {
  feat: "Features",
  fix: "Fixes",
  perf: "Performance",
  revert: "Reverts",
};

const HEADER = "# Changelog\n\nAll notable changes to this component are documented here.\nThe format follows Keep a Changelog; versions follow SemVer.\n";

export function renderSection(entry: PlanEntry, date: string): string {
  const lines: string[] = [`## [${entry.toVersion}] - ${date}`, ""];
  const buckets = new Map<string, string[]>();
  for (const c of entry.commits) {
    const bucket = c.type ? BUCKETS[c.type] : undefined;
    if (!bucket) continue;
    const list = buckets.get(bucket) ?? [];
    list.push(`- ${c.subject} (${c.sha.slice(0, 7)})${c.breaking ? " **breaking**" : ""}`);
    buckets.set(bucket, list);
  }
  for (const name of Object.values(BUCKETS)) {
    const items = buckets.get(name);
    if (!items?.length) continue;
    lines.push(`### ${name}`, "", ...items, "");
  }
  if (entry.propagatedFrom.length > 0) {
    lines.push("### Dependencies", "");
    for (const from of entry.propagatedFrom.sort()) {
      lines.push(`- Bumped for changes in \`${from}\``);
    }
    lines.push("");
  }
  if (buckets.size === 0 && entry.propagatedFrom.length === 0) {
    lines.push("### Changed", "", "- Maintenance release.", "");
  }
  return lines.join("\n").replace(/\n{3,}/g, "\n\n");
}

/**
 * Insert or replace the `## [version]` section in `text`. Existing content is
 * preserved; a section for the same version is replaced in place.
 */
export function upsertSection(text: string, version: string, section: string): string {
  if (!text.trim()) text = HEADER;
  const start = new RegExp(
    `^## \\[${version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\]`,
    "m",
  );
  const lines = text.split("\n");
  const at = lines.findIndex((l) => start.test(l));
  if (at === -1) {
    // Insert before the first existing `## [` section, else append.
    const first = lines.findIndex((l) => /^## \[/.test(l));
    const block = section.trimEnd() + "\n";
    if (first === -1) return text.trimEnd() + "\n\n" + block;
    return [...lines.slice(0, first), block, ...lines.slice(first)].join("\n");
  }
  // Replace the existing section (up to the next `## ` heading or EOF).
  let end = lines.length;
  for (let i = at + 1; i < lines.length; i++) {
    if (/^## /.test(lines[i])) {
      end = i;
      break;
    }
  }
  return [...lines.slice(0, at), section.trimEnd(), "", ...lines.slice(end)].join("\n").replace(/\n{3,}/g, "\n\n");
}

/** Section body for a tag annotation — the same text, minus nothing. */
export function annotationBody(entry: PlanEntry, date: string): string {
  return renderSection(entry, date);
}

/** Root consolidation section linking each bumped component's tag. */
export function renderRootSection(entries: PlanEntry[], date: string): string {
  const bumped = entries.filter((e) => e.fromVersion !== e.toVersion);
  const lines = [`## Release ${date}`, ""];
  for (const e of bumped) {
    const tag = `${e.component}-v${e.toVersion}`;
    lines.push(`- \`${e.component}\` ${e.fromVersion} → ${e.toVersion} (tag \`${tag}\`)`);
  }
  return lines.join("\n") + "\n";
}
