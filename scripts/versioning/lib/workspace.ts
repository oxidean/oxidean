/**
 * Workspace enumeration + version carriers — the filesystem half of `check`
 * (BR1.1 literal carriers, BR1.2 bidirectional completeness, BR1.5
 * non-component exclusion). `loadRegistry` never calls this; only `check`
 * enumerates, which is why enumeration lives in its own module.
 */
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { isSemver } from "./registry.ts";

export interface WorkspaceMembers {
  /** Repo-relative manifest paths of all Cargo workspace members. */
  cargo: string[];
  /** Repo-relative package.json paths of all Bun workspace packages. */
  bun: string[];
}

/** Path segments whose manifests can never be versioned components (BR1.5). */
const NON_COMPONENT_MARKERS = ["templates", "fixtures", "stack-presets", "node_modules"];

export function isNonComponentPath(relPath: string): boolean {
  const parts = relPath.split("/");
  return NON_COMPONENT_MARKERS.some((m) => parts.includes(m));
}

/** `cargo metadata` member manifests (repo-relative). Throws on cargo failure. */
export function cargoMembers(rootDir: string): string[] {
  const proc = Bun.spawnSync(["cargo", "metadata", "--no-deps", "--format-version", "1"], {
    cwd: rootDir,
    stdout: "pipe",
    stderr: "pipe",
  });
  if (proc.exitCode !== 0) {
    throw new Error(`cargo metadata failed: ${proc.stderr.toString().slice(0, 400)}`);
  }
  const meta = JSON.parse(proc.stdout.toString()) as {
    packages: { manifest_path: string }[];
  };
  return meta.packages
    .map((p) => relative(rootDir, p.manifest_path))
    .map((p) => p.split(sep).join("/"))
    .sort();
}

/** Bun workspace packages: expand root package.json `workspaces` globs. */
export function bunMembers(rootDir: string, readRoot = readFileSync): string[] {
  const rootPkg = JSON.parse(readRoot(join(rootDir, "package.json"), "utf-8")) as {
    workspaces?: string[];
  };
  const out: string[] = [];
  for (const pattern of rootPkg.workspaces ?? []) {
    // Workspace patterns are `dir/*` or `dir` style — expand one level of `*`.
    const star = pattern.indexOf("*");
    if (star === -1) {
      const p = join(rootDir, pattern, "package.json");
      if (existsSync(p)) out.push(relative(rootDir, p).split(sep).join("/"));
      continue;
    }
    const base = join(rootDir, pattern.slice(0, star).replace(/\/$/, ""));
    if (!existsSync(base)) continue;
    for (const entry of readdirSync(base, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const p = join(base, entry.name, "package.json");
      if (existsSync(p)) out.push(relative(rootDir, p).split(sep).join("/"));
    }
  }
  return out.sort();
}

export interface CarrierRead {
  ok: boolean;
  version?: string;
  error?: string;
}

/**
 * Read the literal SemVer from a manifest — `package.json` `"version"` or
 * Cargo.toml `[package] version`. `version.workspace = true` and any other
 * non-literal form is a carrier violation (BR1.1).
 */
export function readManifestVersion(rootDir: string, relPath: string): CarrierRead {
  const abs = join(rootDir, relPath);
  if (!existsSync(abs)) return { ok: false, error: `file not found: ${relPath}` };
  const text = readFileSync(abs, "utf-8");
  if (relPath.endsWith("package.json")) {
    const version = (JSON.parse(text) as { version?: unknown }).version;
    if (typeof version !== "string" || !isSemver(version)) {
      return { ok: false, error: `${relPath} "version" must be a literal semver, got ${JSON.stringify(version)}` };
    }
    return { ok: true, version };
  }
  if (relPath.endsWith("Cargo.toml")) {
    const pkg = (Bun.TOML.parse(text) as { package?: { version?: unknown } }).package;
    const version = pkg?.version;
    if (typeof version !== "string") {
      return {
        ok: false,
        error: `${relPath} [package] version must be a literal semver string (workspace-inherited or missing versions are not allowed)`,
      };
    }
    if (!isSemver(version)) {
      return { ok: false, error: `${relPath} version "${version}" is not valid semver` };
    }
    return { ok: true, version };
  }
  if (relPath.endsWith("Chart.yaml")) {
    // Helm chart carrier — the top-level `version:` scalar (NOT `appVersion:`,
    // which is stamped from app_component at bump time). Read line-wise to
    // avoid taking a YAML dependency.
    const m = text.match(/^version:[ \t]*"?([^"\s]+)"?[ \t]*$/m);
    if (!m) return { ok: false, error: `${relPath}: no top-level "version:" field` };
    if (!isSemver(m[1])) {
      return { ok: false, error: `${relPath} version "${m[1]}" is not valid semver` };
    }
    return { ok: true, version: m[1] };
  }
  return { ok: false, error: `unsupported manifest type: ${relPath}` };
}

const CONST_RES = (name: string) => [
  // Rust: pub const NAME: &str = "x.y.z";
  new RegExp(`pub\\s+const\\s+${name}\\s*:\\s*&str\\s*=\\s*"([^"]+)"`),
  // TS/JS: export const NAME = "x.y.z";
  new RegExp(`(?:export\\s+)?const\\s+${name}\\s*=\\s*"([^"]+)"`),
];

/** Resolve a `file#NAME` contract carrier to its literal semver. */
export function readConstant(rootDir: string, constantPath: string): CarrierRead {
  const hash = constantPath.lastIndexOf("#");
  if (hash === -1) return { ok: false, error: `constant_path must be "file#NAME": ${constantPath}` };
  const file = constantPath.slice(0, hash);
  const name = constantPath.slice(hash + 1);
  const abs = join(rootDir, file);
  if (!existsSync(abs)) return { ok: false, error: `file not found: ${file}` };
  const text = readFileSync(abs, "utf-8");
  for (const re of CONST_RES(name)) {
    const m = text.match(re);
    if (m) {
      if (!isSemver(m[1])) {
        return { ok: false, error: `${constantPath} value "${m[1]}" is not valid semver` };
      }
      return { ok: true, version: m[1] };
    }
  }
  return { ok: false, error: `constant ${name} not found in ${file}` };
}
