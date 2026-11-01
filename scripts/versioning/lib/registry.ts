/**
 * versioning.toml loader + validator — contract C1 (`schema: versioning-toml`).
 *
 * `loadRegistry` is the cheap library call every unit consumes: read → parse →
 * validate (schema + entry semantics). Workspace enumeration is NOT part of
 * loading; completeness (BR1.2) is check-only so `load_registry` stays
 * hermetic. Invalid input returns violations — never a partial registry.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { globsMayOverlap } from "./globs.ts";

export const REGISTRY_FILENAME = "versioning.toml";

export const COMPONENT_KINDS = ["crate", "bun", "contract", "helm"] as const;
export type ComponentKind = (typeof COMPONENT_KINDS)[number];

export interface RegistryEntry {
  id: string;
  kind: ComponentKind;
  manifest_path?: string;
  constant_path?: string;
  tag_prefix: string;
  changelog_path: string;
  globs: string[];
  scopes: string[];
  depends_on: string[];
  /** kind=helm only: the component whose version stamps Chart.yaml appVersion. */
  app_component?: string;
  /** kind=helm only: values.yaml `<svc>.image.tag` → component id map. */
  image_components?: Record<string, string>;
}

export interface RegistryConfig {
  /** 1 — additive schema growth is deliberate, never accidental. */
  version: number;
  /** sha40, or the literal "pending" while the merge records its own sha. */
  baseline_commit: string;
  allowed_types: string[];
  reserved_scopes: string[];
  entries: RegistryEntry[];
}

export interface Violation {
  /** Repo-relative path of the offending file (usually `versioning.toml`). */
  file: string;
  line: number;
  message: string;
}

export type LoadResult =
  | { ok: true; config: RegistryConfig }
  | { ok: false; violations: Violation[] };

// ---------------------------------------------------------------------------
// Line index — TOML parse drops positions, so violations locate their key by
// scanning the raw text. `key "<path>"` → 1-based line of the assignment.
// ---------------------------------------------------------------------------

interface LineIndex {
  lines: string[];
  /** component[i] starts at the line of its `[[component]]` header. */
  componentHeaders: number[];
}

function buildLineIndex(text: string): LineIndex {
  const lines = text.split("\n");
  const componentHeaders: number[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (/^\s*\[\[component\]\]\s*$/.test(lines[i])) componentHeaders.push(i + 1);
  }
  return { lines, componentHeaders };
}

/** First line matching `re`, or `fallback` when absent. */
function lineOf(idx: LineIndex, re: RegExp, from = 1, to = idx.lines.length): number {
  for (let i = from - 1; i < to; i++) {
    if (re.test(idx.lines[i])) return i + 1;
  }
  return from;
}

/** Line of `key =` inside the i-th `[[component]]` block (or the block header). */
function entryLine(idx: LineIndex, entryIdx: number, key?: string): number {
  const header = idx.componentHeaders[entryIdx] ?? 1;
  if (!key) return header;
  const end = idx.componentHeaders[entryIdx + 1] ?? idx.lines.length;
  return lineOf(idx, new RegExp(`^\\s*${key}\\s*=`), header + 1, end);
}

const SHA40 = /^[0-9a-f]{40}$/;
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;
const ID_RE = /^[a-z0-9][a-z0-9-]*$/;
const SCOPE_RE = /^[a-z0-9][a-z0-9._-]*$/;
const CONSTANT_PATH_RE = /^(.+)#([A-Za-z_][A-Za-z0-9_]*)$/;

export function isSemver(value: string): boolean {
  return SEMVER.test(value);
}

// ---------------------------------------------------------------------------
// Parse + schema validation (BR1.3: unknown keys are violations anywhere).
// ---------------------------------------------------------------------------

type Raw = Record<string, unknown>;

const TOP_KEYS = new Set(["version", "baseline", "grammar", "component"]);
const BASELINE_KEYS = new Set(["commit"]);
const GRAMMAR_KEYS = new Set(["allowed_types", "reserved_scopes", "breaking_markers"]);
const COMPONENT_KEYS = new Set([
  "id",
  "kind",
  "manifest_path",
  "constant_path",
  "tag_prefix",
  "changelog_path",
  "globs",
  "scopes",
  "depends_on",
  "app_component",
  "image_components",
]);

function unknownKeys(raw: Raw, allowed: Set<string>): string[] {
  return Object.keys(raw).filter((k) => !allowed.has(k));
}

function isRecord(v: unknown): v is Raw {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function isStringArray(v: unknown): v is string[] {
  return Array.isArray(v) && v.every((x) => typeof x === "string");
}

/**
 * Parse `versioning.toml` text and validate schema + entry semantics.
 * Returns violations with file-relative line numbers; no filesystem access.
 */
export function parseRegistry(text: string, file = REGISTRY_FILENAME): LoadResult {
  const violations: Violation[] = [];
  const push = (line: number, message: string) =>
    violations.push({ file, line, message });
  const idx = buildLineIndex(text);

  let raw: Raw;
  try {
    raw = Bun.TOML.parse(text) as Raw;
  } catch (e) {
    push(1, `invalid TOML: ${e instanceof Error ? e.message : String(e)}`);
    return { ok: false, violations };
  }

  for (const k of unknownKeys(raw, TOP_KEYS)) {
    push(lineOf(idx, new RegExp(`^\\s*${k}\\s*=`)), `unknown top-level key "${k}" (BR1.3 strict schema)`);
  }

  if (raw.version !== 1) {
    push(lineOf(idx, /^\s*version\s*=/), `version must be 1 (schema versioning-toml v1)`);
  }

  // --- [baseline] -----------------------------------------------------------
  const baseline = raw.baseline;
  let baseline_commit = "";
  if (!isRecord(baseline)) {
    push(lineOf(idx, /^\s*\[baseline\]/), `missing or malformed [baseline] section`);
  } else {
    for (const k of unknownKeys(baseline, BASELINE_KEYS)) {
      push(lineOf(idx, new RegExp(`^\\s*${k}\\s*=`, undefined), lineOf(idx, /^\s*\[baseline\]/)), `unknown baseline key "${k}" (BR1.3)`);
    }
    if (typeof baseline.commit !== "string" || baseline.commit === "") {
      push(lineOf(idx, /^\s*commit\s*=/, lineOf(idx, /^\s*\[baseline\]/)), `baseline.commit is required (BR1.6)`);
    } else {
      baseline_commit = baseline.commit;
      if (baseline_commit !== "pending" && !SHA40.test(baseline_commit)) {
        push(
          lineOf(idx, /^\s*commit\s*=/, lineOf(idx, /^\s*\[baseline\]/)),
          `baseline.commit must be a 40-hex sha or "pending", got "${baseline_commit}" (BR1.6)`,
        );
      }
    }
  }

  // --- [grammar] ------------------------------------------------------------
  const grammar = raw.grammar;
  let allowed_types: string[] = [];
  let reserved_scopes: string[] = [];
  if (!isRecord(grammar)) {
    push(lineOf(idx, /^\s*\[grammar\]/), `missing or malformed [grammar] section`);
  } else {
    const gLine = lineOf(idx, /^\s*\[grammar\]/);
    for (const k of unknownKeys(grammar, GRAMMAR_KEYS)) {
      push(lineOf(idx, new RegExp(`^\\s*${k}\\s*=`), gLine), `unknown grammar key "${k}" (BR1.3)`);
    }
    if (!isStringArray(grammar.allowed_types) || grammar.allowed_types.length === 0) {
      push(lineOf(idx, /^\s*allowed_types\s*=/, gLine), `grammar.allowed_types must be a non-empty string array`);
    } else allowed_types = grammar.allowed_types;
    if (!isStringArray(grammar.reserved_scopes)) {
      push(lineOf(idx, /^\s*reserved_scopes\s*=/, gLine), `grammar.reserved_scopes must be a string array`);
    } else reserved_scopes = grammar.reserved_scopes;
    if (
      grammar.breaking_markers !== undefined &&
      !isStringArray(grammar.breaking_markers)
    ) {
      push(lineOf(idx, /^\s*breaking_markers\s*=/, gLine), `grammar.breaking_markers must be a string array`);
    }
  }

  // --- [[component]] ---------------------------------------------------------
  const components = raw.component;
  const entries: RegistryEntry[] = [];
  if (!Array.isArray(components) || components.length === 0) {
    push(1, `at least one [[component]] entry is required`);
  } else {
    components.forEach((c, i) => {
      const at = `component[${i}]`;
      if (!isRecord(c)) {
        push(entryLine(idx, i), `${at}: entry must be a table`);
        return;
      }
      for (const k of unknownKeys(c, COMPONENT_KEYS)) {
        push(entryLine(idx, i, k), `${at}: unknown key "${k}" (BR1.3)`);
      }
      const id = c.id;
      if (typeof id !== "string" || !ID_RE.test(id)) {
        push(entryLine(idx, i, "id"), `${at}: id must be a lowercase path segment, got ${JSON.stringify(id)}`);
        return;
      }
      const kind = c.kind;
      if (typeof kind !== "string" || !(COMPONENT_KINDS as readonly string[]).includes(kind)) {
        push(entryLine(idx, i, "kind"), `${at}: kind must be one of ${COMPONENT_KINDS.join("|")}, got ${JSON.stringify(kind)}`);
        return;
      }
      // BR1.1 — kind determines the version carrier, exactly one of the paths.
      const hasManifest = typeof c.manifest_path === "string" && c.manifest_path !== "";
      const hasConstant = typeof c.constant_path === "string" && c.constant_path !== "";
      if (kind === "contract") {
        if (!hasConstant) push(entryLine(idx, i), `${at} ("${id}"): kind=contract requires constant_path`);
        if (hasManifest) push(entryLine(idx, i, "manifest_path"), `${at} ("${id}"): kind=contract must not carry manifest_path (BR1.1)`);
      } else {
        if (!hasManifest) push(entryLine(idx, i), `${at} ("${id}"): kind=${kind} requires manifest_path`);
        if (hasConstant) push(entryLine(idx, i, "constant_path"), `${at} ("${id}"): kind=${kind} must not carry constant_path (BR1.1)`);
      }
      if (hasManifest && typeof c.manifest_path === "string" && c.manifest_path.startsWith("/")) {
        push(entryLine(idx, i, "manifest_path"), `${at} ("${id}"): manifest_path must be repo-relative`);
      }
      if (hasConstant && typeof c.constant_path === "string" && !CONSTANT_PATH_RE.test(c.constant_path)) {
        push(entryLine(idx, i, "constant_path"), `${at} ("${id}"): constant_path must be "file#NAME", got "${c.constant_path}"`);
      }
      // kind=helm carriers: Chart.yaml `version:` + `appVersion:` stamped
      // from `app_component`; values.yaml image tags from `image_components`.
      const hasAppComponent = typeof c.app_component === "string" && c.app_component !== "";
      const hasImageComponents = c.image_components !== undefined;
      if (kind === "helm") {
        if (!hasAppComponent) {
          push(entryLine(idx, i), `${at} ("${id}"): kind=helm requires app_component (the component stamped into Chart.yaml appVersion)`);
        }
        if (hasImageComponents && !isRecord(c.image_components)) {
          push(entryLine(idx, i, "image_components"), `${at} ("${id}"): image_components must be a table of <svc> = <component id>`);
        } else if (isRecord(c.image_components)) {
          for (const [svc, comp] of Object.entries(c.image_components)) {
            if (!ID_RE.test(svc)) {
              push(entryLine(idx, i, "image_components"), `${at} ("${id}"): image_components key "${svc}" is not a lowercase path segment`);
            }
            if (typeof comp !== "string" || !ID_RE.test(comp)) {
              push(entryLine(idx, i, "image_components"), `${at} ("${id}"): image_components.${svc} must be a component id`);
            }
          }
        }
      } else {
        if (c.app_component !== undefined) {
          push(entryLine(idx, i, "app_component"), `${at} ("${id}"): kind=${kind} must not carry app_component (BR1.3)`);
        }
        if (hasImageComponents) {
          push(entryLine(idx, i, "image_components"), `${at} ("${id}"): kind=${kind} must not carry image_components (BR1.3)`);
        }
      }
      if (typeof c.tag_prefix !== "string" || !c.tag_prefix.endsWith("-v")) {
        push(entryLine(idx, i, "tag_prefix"), `${at} ("${id}"): tag_prefix must end in "-v"`);
      }
      if (typeof c.changelog_path !== "string" || c.changelog_path === "") {
        push(entryLine(idx, i, "changelog_path"), `${at} ("${id}"): changelog_path is required`);
      }
      for (const arrKey of ["globs", "scopes", "depends_on"] as const) {
        if (!isStringArray(c[arrKey])) {
          push(entryLine(idx, i, arrKey), `${at} ("${id}"): ${arrKey} must be a string array`);
        }
      }
      const scopes = isStringArray(c.scopes) ? c.scopes : [];
      for (const s of scopes) {
        if (!SCOPE_RE.test(s)) push(entryLine(idx, i, "scopes"), `${at} ("${id}"): invalid scope token "${s}"`);
      }
      entries.push({
        id,
        kind: kind as ComponentKind,
        manifest_path: hasManifest ? (c.manifest_path as string) : undefined,
        constant_path: hasConstant ? (c.constant_path as string) : undefined,
        tag_prefix: typeof c.tag_prefix === "string" ? c.tag_prefix : "",
        changelog_path: typeof c.changelog_path === "string" ? c.changelog_path : "",
        globs: isStringArray(c.globs) ? c.globs : [],
        scopes,
        depends_on: isStringArray(c.depends_on) ? c.depends_on : [],
        app_component: hasAppComponent ? (c.app_component as string) : undefined,
        image_components: isRecord(c.image_components)
          ? (c.image_components as Record<string, string>)
          : undefined,
      });
    });
  }

  // --- cross-entry validation (BR1.4, BR1.7) ---------------------------------
  const byId = new Map<string, number>();
  entries.forEach((e, i) => {
    const prev = byId.get(e.id);
    if (prev !== undefined) {
      push(entryLine(idx, i, "id"), `duplicate component id "${e.id}" (first at line ${entryLine(idx, prev, "id")})`);
    } else byId.set(e.id, i);
  });

  const tagOwners = new Map<string, string>();
  const scopeOwners = new Map<string, string>();
  entries.forEach((e, i) => {
    if (tagOwners.has(e.tag_prefix)) {
      push(entryLine(idx, i, "tag_prefix"), `tag_prefix "${e.tag_prefix}" collides with component "${tagOwners.get(e.tag_prefix)}" (BR1.7)`);
    } else tagOwners.set(e.tag_prefix, e.id);
    for (const s of e.scopes) {
      if (reserved_scopes.includes(s)) {
        push(entryLine(idx, i, "scopes"), `scope "${s}" is reserved (BR1.7)`);
      }
      if (scopeOwners.has(s)) {
        push(entryLine(idx, i, "scopes"), `scope alias "${s}" collides with component "${scopeOwners.get(s)}" (BR1.7)`);
      } else scopeOwners.set(s, e.id);
    }
  });

  // glob overlap — conservative segment-wise may-overlap (BR1.7)
  for (let i = 0; i < entries.length; i++) {
    for (let j = i + 1; j < entries.length; j++) {
      for (const ga of entries[i].globs) {
        for (const gb of entries[j].globs) {
          if (globsMayOverlap(ga, gb)) {
            push(
              entryLine(idx, j, "globs"),
              `glob "${gb}" (${entries[j].id}) may overlap "${ga}" (${entries[i].id}) — a changed file must attribute to at most one entry (BR1.7)`,
            );
          }
        }
      }
    }
  }

  // depends_on — known ids, no self-edge, acyclic (BR1.4)
  entries.forEach((e, i) => {
    for (const dep of e.depends_on) {
      if (dep === e.id) {
        push(entryLine(idx, i, "depends_on"), `"${e.id}" cannot depend on itself (BR1.4)`);
      } else if (!byId.has(dep)) {
        push(entryLine(idx, i, "depends_on"), `"${e.id}" depends on unknown component "${dep}" (BR1.4)`);
      }
    }
    // kind=helm linkages: the stamped components must be real and declared in
    // depends_on so their releases propagate a chart bump (fresh stamps).
    if (e.kind === "helm") {
      if (e.app_component && !byId.has(e.app_component)) {
        push(entryLine(idx, i, "app_component"), `"${e.id}": app_component "${e.app_component}" is not a registered component (BR1.4)`);
      } else if (e.app_component && !e.depends_on.includes(e.app_component)) {
        push(entryLine(idx, i, "app_component"), `"${e.id}": app_component "${e.app_component}" must appear in depends_on so its releases refresh the stamp (BR1.4)`);
      }
      for (const [svc, comp] of Object.entries(e.image_components ?? {})) {
        if (!byId.has(comp)) {
          push(entryLine(idx, i, "image_components"), `"${e.id}": image_components.${svc} references unknown component "${comp}" (BR1.4)`);
        } else if (!e.depends_on.includes(comp)) {
          push(entryLine(idx, i, "image_components"), `"${e.id}": image_components.${svc} = "${comp}" must appear in depends_on (BR1.4)`);
        }
      }
    }
  });
  const cycle = findCycle(entries);
  if (cycle) {
    const last = byId.get(cycle[cycle.length - 1]);
    push(last !== undefined ? entryLine(idx, last, "depends_on") : 1, `dependency cycle: ${cycle.join(" -> ")} (BR1.4)`);
  }

  if (violations.length > 0) return { ok: false, violations };
  return {
    ok: true,
    config: {
      version: 1,
      baseline_commit,
      allowed_types,
      reserved_scopes,
      entries,
    },
  };
}

/** Detect one dependency cycle via DFS; returns the cycle ids or null. */
function findCycle(entries: RegistryEntry[]): string[] | null {
  const edges = new Map(entries.map((e) => [e.id, e.depends_on]));
  const state = new Map<string, "visiting" | "done">();
  const stack: string[] = [];
  const visit = (id: string): string[] | null => {
    state.set(id, "visiting");
    stack.push(id);
    for (const dep of edges.get(id) ?? []) {
      if (!edges.has(dep)) continue;
      if (state.get(dep) === "visiting") {
        return [...stack.slice(stack.indexOf(dep)), dep];
      }
      if (!state.has(dep)) {
        const found = visit(dep);
        if (found) return found;
      }
    }
    stack.pop();
    state.set(id, "done");
    return null;
  };
  for (const e of entries) {
    if (!state.has(e.id)) {
      const found = visit(e.id);
      if (found) return found;
    }
  }
  return null;
}

/**
 * Load and validate `versioning.toml` from `rootDir`.
 * Returns violations on ANY schema/semantic error — never a partial registry.
 * Completeness against the live workspace is intentionally not checked here
 * (BR1.2 is enforced by `check`, locally via `make version-check` and in CI).
 */
export function loadRegistry(rootDir: string): LoadResult {
  const path = join(rootDir, REGISTRY_FILENAME);
  let text: string;
  try {
    text = readFileSync(path, "utf-8");
  } catch {
    return {
      ok: false,
      violations: [{ file: REGISTRY_FILENAME, line: 1, message: "registry not found" }],
    };
  }
  return parseRegistry(text);
}
