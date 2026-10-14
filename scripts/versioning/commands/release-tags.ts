/**
 * `release-tags` — annotated component tags per C4 (WF-2, FR3.4/FR2.5).
 *
 *   release-tags [--since <ref>] [--dry-run]   tag every bumped plan entry
 *   release-tags --all [--dry-run]             tag every component at its
 *                                              carrier version (baseline cut)
 *
 * Tags are annotated (`git tag -a`) with the changelog section as the body.
 * Existing tags are skipped; creation runs at HEAD. Pushing is the caller's
 * job — and must never ride the default GITHUB_TOKEN (FR3.4).
 */
import { execFileSync } from "node:child_process";
import { annotationBody } from "../lib/changelog.ts";
import { carrierVersion } from "../lib/plan.ts";
import { buildPlan } from "./plan.ts";

export const usage =
  "release-tags [--since <ref>] [--all] [--dry-run]\n" +
  "                                create annotated <component>-v<semver> tags";

interface TagTarget {
  component: string;
  tag: string;
  version: string;
  annotation: string;
}

function existingTags(root: string): Set<string> {
  const out = execFileSync("git", ["tag", "-l"], {
    cwd: root,
    encoding: "utf-8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return new Set(out.split("\n").map((t) => t.trim()).filter(Boolean));
}

function createTag(root: string, tag: string, message: string): void {
  // --cleanup=verbatim: changelog sections start with `##`, which git would
  // otherwise strip as comment lines from the annotation.
  execFileSync("git", ["tag", "-a", "--cleanup=verbatim", "-F", "-", tag], {
    cwd: root,
    input: message,
    stdio: ["pipe", "pipe", "pipe"],
  });
}

export function releaseTagsCommand(root: string, args: string[]): number {
  const all = args.includes("--all");
  const dryRun = args.includes("--dry-run");
  const rest = args.filter((a) => a !== "--all" && a !== "--dry-run");

  const built = buildPlan(root, rest);
  if (!built.ok) return built.code;
  const { plan, registry } = built;
  const date = new Date().toISOString().slice(0, 10);

  const targets: TagTarget[] = [];
  if (all) {
    for (const e of registry.entries) {
      const version = carrierVersion(root, e);
      targets.push({
        component: e.id,
        tag: `${e.tag_prefix}${version}`,
        version,
        annotation: `## [${version}] - ${date}\n\nBaseline tag for the ${e.id} component.\n`,
      });
    }
  } else {
    for (const entry of plan.entries) {
      if (entry.fromVersion === entry.toVersion) continue;
      const reg = registry.entries.find((c) => c.id === entry.component);
      if (!reg) continue;
      targets.push({
        component: entry.component,
        tag: `${reg.tag_prefix}${entry.toVersion}`,
        version: entry.toVersion,
        annotation: annotationBody(entry, date),
      });
    }
  }

  const existing = existingTags(root);
  const created: string[] = [];
  const skipped: string[] = [];
  for (const t of targets) {
    if (existing.has(t.tag)) {
      skipped.push(t.tag);
      continue;
    }
    if (!dryRun) createTag(root, t.tag, t.annotation);
    created.push(t.tag);
  }

  for (const t of skipped) {
    process.stdout.write(`release-tags: ${t} already exists — skipped\n`);
  }
  if (created.length === 0) {
    process.stdout.write("release-tags: nothing to tag\n");
    return 0;
  }
  process.stdout.write(
    `${dryRun ? "release-tags (dry-run)" : "release-tags"}: ${created.length} tag(s):\n` +
      created.map((t) => `  ${t}`).join("\n") +
      "\n",
  );
  return 0;
}
