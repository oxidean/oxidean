/**
 * `check` subcommand — full registry validation including workspace
 * completeness (BR1.2). Dual-format output per BR1.8: `file:line: message`
 * lines on stderr, or `--json` for CI annotation. Exit 0 clean (or warnings
 * only), 1 violations, 2 usage/IO error.
 */
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, sep } from "node:path";
import {
  loadRegistry,
  REGISTRY_FILENAME,
  type RegistryConfig,
  type Violation,
} from "../lib/registry.ts";
import {
  bunMembers,
  cargoMembers,
  isNonComponentPath,
  readConstant,
  readManifestVersion,
  type WorkspaceMembers,
} from "../lib/workspace.ts";

export const usage =
  "check [--json]                    validate versioning.toml + workspace completeness";

export interface CheckResult {
  violations: Violation[];
  warnings: string[];
  entries: number;
}

export interface CheckDeps {
  /** Injectable for tests — default shells out to `cargo metadata`. */
  enumerate?: (rootDir: string) => WorkspaceMembers;
}

function enumerate(rootDir: string): WorkspaceMembers {
  return { cargo: cargoMembers(rootDir), bun: bunMembers(rootDir) };
}

/** Line of `id = "<entryId>"` in the raw registry text (for `file:line` output). */
function entryLineResolver(text: string): (id: string) => number {
  const lines = text.split("\n");
  return (id: string) => {
    const re = new RegExp(`^\\s*id\\s*=\\s*"${id}"\\s*$`);
    const hit = lines.findIndex((l) => re.test(l));
    return hit === -1 ? 1 : hit + 1;
  };
}

/** Filesystem + workspace validation, given a schema-valid config. */
export function validateWorkspace(
  rootDir: string,
  config: RegistryConfig,
  deps: CheckDeps = {},
): { violations: Violation[]; warnings: string[] } {
  const violations: Violation[] = [];
  const warnings: string[] = [];
  let lineFor = (_id: string) => 1;
  try {
    lineFor = entryLineResolver(readFileSync(join(rootDir, REGISTRY_FILENAME), "utf-8"));
  } catch {
    // Loader already reported a missing/unreadable file; keep line 1.
  }
  const push = (line: number, message: string) =>
    violations.push({ file: REGISTRY_FILENAME, line, message });

  // BR1.6 — pending baseline warns but passes; scans fall back to repo root.
  if (config.baseline_commit === "pending") {
    warnings.push(
      `baseline.commit is "pending" — record the merge sha in a follow-up chore(release): commit (BR1.6)`,
    );
  }

  const manifestOwners = new Map<string, string>();
  config.entries.forEach((e) => {
    const line = lineFor(e.id);
    const tag = `component "${e.id}"`;
    // BR1.5 — a version carrier can never live under a non-component path.
    if (e.manifest_path && isNonComponentPath(e.manifest_path)) {
      push(line, `${tag}: manifest_path "${e.manifest_path}" targets a non-component path (templates/fixtures/presets) (BR1.5)`);
    }
    if (e.manifest_path) {
      if (manifestOwners.has(e.manifest_path)) {
        push(line, `${tag}: manifest_path "${e.manifest_path}" is already claimed by "${manifestOwners.get(e.manifest_path)}"`);
      } else manifestOwners.set(e.manifest_path, e.id);
      const carrier = readManifestVersion(rootDir, e.manifest_path);
      if (!carrier.ok) push(line, `${tag}: ${carrier.error} (BR1.1)`);
    }
    if (e.constant_path) {
      const carrier = readConstant(rootDir, e.constant_path);
      if (!carrier.ok) push(line, `${tag}: ${carrier.error} (BR1.1)`);
    }
    
  });

  // BR1.2 — bidirectional completeness.
  let members: WorkspaceMembers;
  try {
    members = (deps.enumerate ?? enumerate)(rootDir);
  } catch (err) {
    violations.push({
      file: REGISTRY_FILENAME,
      line: 1,
      message: `workspace enumeration failed: ${err instanceof Error ? err.message : String(err)}`,
    });
    return { violations, warnings };
  }
  const workspaceManifests = new Set([...members.cargo, ...members.bun]);
  for (const manifest of workspaceManifests) {
    if (!manifestOwners.has(manifest)) {
      push(1, `workspace member "${manifest}" has no registry entry (BR1.2)`);
    }
  }
  for (const e of config.entries) {
    // kind=helm carriers live outside both workspaces (deploy/helm/**) — the
    // bidirectional-membership rule applies to crate/bun manifests only.
    if (e.manifest_path && e.kind !== "helm" && !workspaceManifests.has(e.manifest_path)) {
      push(lineFor(e.id), `component "${e.id}" manifest_path "${e.manifest_path}" is not a workspace member (BR1.2)`);
    }
  }

  // A `version` pin on a path dep targeting a workspace member breaks release
  // cuts: `bump` rewrites the member's version and `cargo metadata` then fails
  // on the stale constraint. External path deps may legitimately keep a pin.
  const cargoManifestSet = new Set(members.cargo);
  for (const rel of [...members.cargo, "Cargo.toml"]) {
    if (!existsSync(join(rootDir, rel))) continue;
    const text = readFileSync(join(rootDir, rel), "utf-8");
    let parsed: Record<string, unknown>;
    try {
      parsed = Bun.TOML.parse(text) as Record<string, unknown>;
    } catch {
      continue; // cargo metadata already validated member manifests
    }
    const depTables: Record<string, unknown>[] = [];
    for (const key of ["dependencies", "dev-dependencies", "build-dependencies"]) {
      const t = parsed[key];
      if (t && typeof t === "object") depTables.push(t as Record<string, unknown>);
    }
    const target = parsed.target;
    if (target && typeof target === "object") {
      for (const cfg of Object.values(target as Record<string, unknown>)) {
        if (!cfg || typeof cfg !== "object") continue;
        for (const key of ["dependencies", "dev-dependencies", "build-dependencies"]) {
          const t = (cfg as Record<string, unknown>)[key];
          if (t && typeof t === "object") depTables.push(t as Record<string, unknown>);
        }
      }
    }
    const wsDeps = (parsed.workspace as Record<string, unknown> | undefined)?.dependencies;
    if (wsDeps && typeof wsDeps === "object") depTables.push(wsDeps as Record<string, unknown>);

    for (const table of depTables) {
      for (const [depName, spec] of Object.entries(table)) {
        if (!spec || typeof spec !== "object") continue;
        const s = spec as Record<string, unknown>;
        if (typeof s.path !== "string" || typeof s.version !== "string") continue;
        const targetRel = join(dirname(rel), s.path, "Cargo.toml").split(sep).join("/");
        if (!cargoManifestSet.has(targetRel)) continue;
        const line = manifestDepLine(text, depName);
        violations.push({
          file: rel,
          line,
          message: `dependency "${depName}" pins version "${s.version}" on workspace path dep "${s.path}" — drop the version key; release bumps rewrite member versions and the stale pin breaks cargo metadata`,
        });
      }
    }
  }
  return { violations, warnings };
}

/** Line of a dependency entry (`name = ...` or dotted `name.key = ...`). */
function manifestDepLine(text: string, depName: string): number {
  const escaped = depName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`^\\s*${escaped}(?:\\.|\\s*=)`);
  const idx = text.split("\n").findIndex((l) => re.test(l));
  return idx === -1 ? 1 : idx + 1;
}

/** Full `check` — schema load + workspace validation against `rootDir`. */
export function runCheck(rootDir: string, deps: CheckDeps = {}): CheckResult {
  const loaded = loadRegistry(rootDir);
  if (!loaded.ok) {
    return { violations: loaded.violations, warnings: [], entries: 0 };
  }
  const { violations, warnings } = validateWorkspace(rootDir, loaded.config, deps);
  return { violations, warnings, entries: loaded.config.entries.length };
}

/** CLI body — returns the exit code; streams output itself. */
export function checkCommand(rootDir: string, argv: string[]): number {
  const json = argv.includes("--json");
  const unknown = argv.filter((a) => a !== "--json");
  if (unknown.length > 0) {
    process.stderr.write(`check: unknown argument(s): ${unknown.join(" ")}\n`);
    return 2;
  }
  let result: CheckResult;
  try {
    result = runCheck(rootDir);
  } catch (err) {
    process.stderr.write(`check: ${err instanceof Error ? err.message : String(err)}\n`);
    return 2;
  }
  if (json) {
    process.stdout.write(
      JSON.stringify(
        {
          violations: result.violations,
          warnings: result.warnings,
          entries: result.entries,
        },
        null,
        2,
      ) + "\n",
    );
  } else {
    for (const v of result.violations) {
      process.stderr.write(`${v.file}:${v.line}: ${v.message}\n`);
    }
    for (const w of result.warnings) {
      process.stderr.write(`warning: ${w}\n`);
    }
    if (result.violations.length === 0) {
      process.stdout.write(
        `versioning: ok — ${result.entries} components registered` +
          (result.warnings.length ? ` (${result.warnings.length} warning${result.warnings.length > 1 ? "s" : ""})` : "") +
          "\n",
      );
    }
  }
  return result.violations.length > 0 ? 1 : 0;
}
