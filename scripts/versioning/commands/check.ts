/**
 * `check` subcommand — full registry validation including workspace
 * completeness (BR1.2). Dual-format output per BR1.8: `file:line: message`
 * lines on stderr, or `--json` for CI annotation. Exit 0 clean (or warnings
 * only), 1 violations, 2 usage/IO error.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
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
    if (e.manifest_path && !workspaceManifests.has(e.manifest_path)) {
      push(lineFor(e.id), `component "${e.id}" manifest_path "${e.manifest_path}" is not a workspace member (BR1.2)`);
    }
  }
  return { violations, warnings };
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
