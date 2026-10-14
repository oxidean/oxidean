/**
 * `detect` — dry-run attribution over a commit range (WF-1, FR3).
 * Prints one line per attribution; `--json` emits the Attribution[] array.
 */
import { loadRegistry, type RegistryConfig } from "../lib/registry.ts";
import { liveGit } from "../lib/git.ts";
import { attributeCommit, collectRange } from "../lib/plan.ts";
import type { Channel } from "../lib/semver.ts";

export const usage =
  "detect [--since <ref>] [--json]   attribute the commit range to components";

export function parseCommonArgs(args: string[]): {
  since?: string;
  channel: Channel;
  json: boolean;
  dryRun: boolean;
  error?: string;
} {
  let since: string | undefined;
  let channel: Channel = "stable";
  let json = false;
  let dryRun = false;
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === "--since") {
      since = args[++i];
      if (since === undefined) return { channel, json, dryRun, error: "--since requires a ref" };
    } else if (a === "--channel") {
      const c = args[++i];
      if (c !== "stable" && c !== "alpha" && c !== "beta" && c !== "nightly") {
        return { channel, json, dryRun, error: `--channel must be stable|alpha|beta|nightly, got "${c}"` };
      }
      channel = c;
    } else if (a === "--json") {
      json = true;
    } else if (a === "--dry-run") {
      dryRun = true;
    } else {
      return { channel, json, dryRun, error: `unknown argument "${a}"` };
    }
  }
  return { since, channel, json, dryRun };
}

export function loadOrFail(root: string): RegistryConfig | null {
  const res = loadRegistry(root);
  if (!res.ok) {
    for (const v of res.violations) {
      process.stderr.write(`${v.file}:${v.line}: ${v.message}\n`);
    }
    return null;
  }
  return res.config;
}

export function detectCommand(root: string, args: string[]): number {
  const flags = parseCommonArgs(args);
  if (flags.error) {
    process.stderr.write(`detect: ${flags.error}\n`);
    return 2;
  }
  const registry = loadOrFail(root);
  if (registry === null) return 1;

  const git = liveGit(root);
  let range;
  try {
    range = collectRange(root, git, registry, flags.since);
  } catch (e) {
    process.stderr.write(`detect: ${e instanceof Error ? e.message : String(e)}\n`);
    return 2;
  }

  const warnings: string[] = [];
  const attributions = range.commits.flatMap((c) => {
    if (c.parsed.exempt || c.parsed.scope_status === "reserved") return [];
    return attributeCommit(
      { ...c.parsed, sha: c.parsed.sha, subject: c.subject },
      c.files,
      registry.entries,
      warnings,
    );
  });

  if (flags.json) {
    process.stdout.write(
      JSON.stringify({ base: range.lower, attributions }, null, 2) + "\n",
    );
  } else {
    if (attributions.length === 0) {
      process.stdout.write("detect: no attributed commits in range\n");
    }
    for (const a of attributions) {
      const tag = a.type ?? (a.breaking ? "breaking" : "unparsed");
      process.stdout.write(
        `${a.sha.slice(0, 10)} ${tag}${a.breaking ? "!" : ""} → ${a.componentId} [${a.source}${a.ambiguous ? ",ambiguous" : ""}] ${a.subject.slice(0, 72)}\n`,
      );
    }
  }
  for (const w of warnings) process.stderr.write(`warn: ${w}\n`);
  return 0;
}
