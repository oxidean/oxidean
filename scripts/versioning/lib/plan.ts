/**
 * Release-plan engine — contract C3. Joins U2 parser signals with U1 registry
 * metadata over a commit range, propagates through the dependency DAG, and
 * computes per-component bumps. Pure except for the injected `Git`/`rootDir`
 * boundaries; output is deterministic modulo `generatedAt` (BR3.8).
 */
import type { Git, GitCommit } from "./git.ts";
import { matchesAny } from "./globs.ts";
import { parseCommit, type ConventionalCommit } from "./commit.ts";
import type { RegistryConfig, RegistryEntry } from "./registry.ts";
import { isSemver } from "./registry.ts";
import { readConstant, readManifestVersion } from "./workspace.ts";
import {
  applyBump,
  bumpForCommit,
  compareSemver,
  maxBump,
  withChannel,
  type BumpLevel,
  type Channel,
} from "./semver.ts";

export type AttributionSource = "scope" | "glob" | "scope_and_glob";

export interface Attribution {
  sha: string;
  componentId: string;
  subject: string;
  /** Absent when the commit was grammar-invalid (counts as patch evidence). */
  type?: string;
  scope?: string;
  breaking: boolean;
  source: AttributionSource;
  /** Path matched more than one entry's globs — warning emitted (defensive). */
  ambiguous: boolean;
}

export interface PlanEntry {
  component: string;
  fromVersion: string;
  toVersion: string;
  bump: BumpLevel;
  direct: boolean;
  propagatedFrom: string[];
  commits: { sha: string; type?: string; scope?: string; breaking: boolean; subject: string }[];
}

export interface BumpPlan {
  schema: "release-plan";
  version: 1;
  generatedAt: string;
  baseRef: string;
  channel: Channel;
  entries: PlanEntry[];
  /** Advisory output — invalid-grammar commits attributed via paths, etc. */
  warnings: string[];
}

/** Reverse adjacency: dependency id → its dependents. */
export function dependentsOf(entries: RegistryEntry[]): Map<string, string[]> {
  const rev = new Map<string, string[]>();
  for (const e of entries) {
    for (const dep of e.depends_on) {
      const list = rev.get(dep) ?? [];
      list.push(e.id);
      rev.set(dep, list);
    }
  }
  return rev;
}

/**
 * WF-1 — attribute one parsed commit to components (BR3.2). `files` are the
 * commit's changed paths. Returns one Attribution per attributed component.
 */
export function attributeCommit(
  parsed: ConventionalCommit & { sha: string; subject: string },
  files: string[],
  entries: RegistryEntry[],
  warnings: string[],
): Attribution[] {
  const byScope = new Set(parsed.resolved);
  const byGlob = new Set<string>();
  const multiMatch = new Set<string>();

  for (const f of files) {
    const matched = entries.filter((e) => matchesAny(f, e.globs));
    for (const e of matched) byGlob.add(e.id);
    // A single path matching multiple entries is ambiguous (defensive — BR1.7
    // normally prevents it at registry level).
    if (matched.length > 1) {
      for (const e of matched) multiMatch.add(e.id);
      warnings.push(
        `${parsed.sha.slice(0, 10)}: path "${f}" matched ${matched.map((e) => e.id).join(", ")} — attributed to all`,
      );
    }
  }

  const ids = new Set([...byScope, ...byGlob]);
  const out: Attribution[] = [];
  for (const id of [...ids].sort()) {
    const scoped = byScope.has(id);
    const globbed = byGlob.has(id);
    out.push({
      sha: parsed.sha,
      componentId: id,
      subject: parsed.subject,
      type: parsed.type,
      scope: parsed.scope,
      breaking: parsed.breaking,
      source: scoped && globbed ? "scope_and_glob" : scoped ? "scope" : "glob",
      ambiguous: multiMatch.has(id),
    });
  }
  return out;
}

/**
 * WF-2 step 2 — dependents closure over `depends_on` (BR3.3). `origins` are
 * components with direct bumps; every reached dependent records the origin
 * ids that reach it (origin→origin edges are recorded as evidence too — they
 * feed FR4.4's Dependencies section without altering bump math).
 */
export function propagate(
  entries: RegistryEntry[],
  origins: Set<string>,
): Map<string, Set<string>> {
  const rev = dependentsOf(entries);
  const propagatedFrom = new Map<string, Set<string>>();
  for (const origin of origins) {
    const queue = [origin];
    const seen = new Set<string>([origin]);
    while (queue.length > 0) {
      const cur = queue.pop()!;
      for (const dep of rev.get(cur) ?? []) {
        if (seen.has(dep)) continue;
        seen.add(dep);
        const set = propagatedFrom.get(dep) ?? new Set<string>();
        set.add(origin);
        propagatedFrom.set(dep, set);
        queue.push(dep);
      }
    }
  }
  // A component never propagates to itself (defensive; BR1.4 already
  // forbids cycles).
  for (const [id, set] of propagatedFrom) set.delete(id);
  return propagatedFrom;
}

/** fromVersion — always from the live carrier, never the registry (BR3.6). */
export function carrierVersion(rootDir: string, e: RegistryEntry): string {
  const r = e.manifest_path
    ? readManifestVersion(rootDir, e.manifest_path)
    : readConstant(rootDir, e.constant_path!);
  if (!r.ok || r.version === undefined) {
    throw new Error(`${e.id}: ${r.error ?? "version carrier unreadable"}`);
  }
  return r.version;
}

export interface RangeCommit {
  parsed: ConventionalCommit & { sha: string };
  subject: string;
  files: string[];
}

/**
 * Compute the release plan over a pre-enumerated range. `componentBounds`
 * (FR3) holds, per component that has released before, the sha set of commits
 * after its latest tag — attributions outside it are dropped.
 */
export function computePlan(
  registry: RegistryConfig,
  commits: RangeCommit[],
  opts: {
    rootDir: string;
    git: Git;
    channel: Channel;
    componentBounds?: Map<string, Set<string>>;
  },
): BumpPlan {
  const warnings: string[] = [];
  const entries = registry.entries;
  const versions = new Map(entries.map((e) => [e.id, carrierVersion(opts.rootDir, e)]));

  const directCommits = new Map<string, PlanEntry["commits"]>();
  const directLevel = new Map<string, BumpLevel>();
  for (const c of commits) {
    const p = c.parsed;
    // Exempt subjects AND reserved scopes produce no attribution (BR2.3/BR3.2 —
    // reserved commits never fall back to globs either).
    if (p.exempt || p.scope_status === "reserved") continue;
    if (!p.valid) {
      warnings.push(
        `${p.sha.slice(0, 10)}: grammar-invalid ("${c.subject.slice(0, 60)}") — attributing via paths at patch level`,
      );
    }
    const attrs = attributeCommit(
      { ...p, sha: p.sha, subject: c.subject },
      c.files,
      entries,
      warnings,
    );
    for (const a of attrs) {
      const bound = opts.componentBounds?.get(a.componentId);
      if (bound && !bound.has(a.sha)) continue;
      const level = bumpForCommit(a.type, a.breaking, versions.get(a.componentId)!);
      const list = directCommits.get(a.componentId) ?? [];
      list.push({ sha: a.sha, type: a.type, scope: a.scope, breaking: a.breaking, subject: a.subject });
      directCommits.set(a.componentId, list);
      directLevel.set(a.componentId, maxBump(directLevel.get(a.componentId) ?? "none", level));
    }
  }

  // Origins = direct bump ≠ none (FR3.2: only *bumped* deps propagate).
  const origins = new Set(
    [...directLevel.entries()].filter(([, l]) => l !== "none").map(([id]) => id),
  );
  const propagatedFrom = propagate(entries, origins);

  const channel = opts.channel;
  const planEntries: PlanEntry[] = [];
  for (const e of entries) {
    const direct = directLevel.get(e.id) ?? "none";
    const prop = propagatedFrom.get(e.id);
    const hasProp = prop !== undefined && prop.size > 0;
    const bump: BumpLevel = hasProp ? maxBump(direct, "patch") : direct;
    if (bump === "none") continue;
    const from = versions.get(e.id)!;
    const base = applyBump(from, bump);
    const prior =
      channel === "stable"
        ? 0
        : opts.git
            .prefixedTags(e.tag_prefix)
            .filter((t) => t.version.startsWith(`${base}-${channel}`)).length;
    planEntries.push({
      component: e.id,
      fromVersion: from,
      toVersion: withChannel(base, channel, prior),
      bump,
      direct: direct !== "none",
      propagatedFrom: prop ? [...prop].sort() : [],
      commits: (directCommits.get(e.id) ?? []).sort((a, b) => a.sha.localeCompare(b.sha)),
    });
  }
  planEntries.sort((a, b) => a.component.localeCompare(b.component));

  return {
    schema: "release-plan",
    version: 1,
    generatedAt: new Date().toISOString(),
    baseRef: opts.git.headSha(),
    channel,
    entries: planEntries,
    warnings,
  };
}

/**
 * Enumerate the live range: global bound (BR3.1) + per-component tag bounds
 * (FR3). Returns commits (newest first) with parsed signals + changed files.
 */
export function collectRange(
  rootDir: string,
  git: Git,
  registry: RegistryConfig,
  since?: string,
): {
  commits: RangeCommit[];
  componentBounds: Map<string, Set<string>>;
  lower: string | null;
} {
  const lower =
    since ??
    (registry.baseline_commit === "pending" ? null : registry.baseline_commit);
  const raw = git.rangeCommits(lower);
  const commits = raw.map((c: GitCommit) => ({
    parsed: {
      ...parseCommit({ subject: c.subject, body: c.body, sha: c.sha }, registry),
      sha: c.sha,
    },
    subject: c.subject,
    files: git.commitFiles(c.sha),
  }));
  const componentBounds = new Map<string, Set<string>>();
  for (const e of registry.entries) {
    const tags = git
      .prefixedTags(e.tag_prefix)
      .filter((t) => isSemver(t.version))
      .sort((a, b) => compareSemver(a.version, b.version));
    if (tags.length === 0) continue;
    componentBounds.set(e.id, git.commitsSince(tags[tags.length - 1].tag));
  }
  return { commits, componentBounds, lower };
}
