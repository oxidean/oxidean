import {
  FG_PERM_CATALOG,
  SCOPE_CATALOG,
  type ClassicPatScope,
  type FgPermLevel,
  type FgPerms,
} from "@oxidean/api-client";

/** Three-way permission picker value — `none` omits the family from `perms`. */
export type PermChoice = "none" | "read" | "write";

type ClassicScopeRow = {
  scope: ClassicPatScope;
  testid: string;
  label: string;
  hint: string;
};

export type ClassicScopeGroup = {
  title: string;
  scopes: ClassicScopeRow[];
};

/** `repo:status` → `scope-repo-status` (stable test ids across catalog edits). */
function scopeTestId(name: string): string {
  return `scope-${name.replace(/[:_]/g, "-")}`;
}

/**
 * Classic scope groups, derived from the generated `SCOPE_CATALOG`
 * (`crates/oxidean-core/src/scope_catalog.rs` via `make rpc-gen`) — same rows,
 * grouping, and implication edges the API enforces.
 */
export const CLASSIC_SCOPE_GROUPS: ClassicScopeGroup[] = (() => {
  const byGroup = new Map<string, ClassicScopeRow[]>();
  for (const s of SCOPE_CATALOG) {
    const row: ClassicScopeRow = {
      scope: s.name as ClassicPatScope,
      testid: scopeTestId(s.name),
      label: `${s.name} — ${s.description}`,
      hint: s.hint,
    };
    const list = byGroup.get(s.group) ?? [];
    list.push(row);
    byGroup.set(s.group, list);
  }
  return [...byGroup.entries()].map(([title, scopes]) => ({ title, scopes }));
})();

/** Broader scope ⇒ scopes it implies (UI greys implied boxes; the API enforces). */
const CLASSIC_SCOPE_IMPLIES: Partial<Record<ClassicPatScope, ClassicPatScope[]>> =
  Object.fromEntries(
    SCOPE_CATALOG.filter((s) => s.implies.length > 0).map((s) => [
      s.name as ClassicPatScope,
      s.implies as ClassicPatScope[],
    ]),
  );

export type ClassicScopeValues = Record<string, boolean>;

/** Every scope `scope` implies — transitively flattened. */
function impliedScopes(scope: ClassicPatScope): ClassicPatScope[] {
  const seen = new Set<ClassicPatScope>();
  const walk = (s: ClassicPatScope) => {
    for (const next of CLASSIC_SCOPE_IMPLIES[s] ?? []) {
      if (seen.has(next)) continue;
      seen.add(next);
      walk(next);
    }
  };
  walk(scope);
  return [...seen];
}

/** True when `scope` is implied by another checked scope (checked + disabled). */
export function isImplied(values: ClassicScopeValues, scope: ClassicPatScope): boolean {
  return (Object.keys(values) as ClassicPatScope[]).some(
    (held) => values[held] === true && impliedScopes(held).includes(scope),
  );
}

/** Scopes to send — checked scopes only (the server applies implications). */
export function selectedScopes(values: ClassicScopeValues): ClassicPatScope[] {
  return (Object.keys(values) as ClassicPatScope[]).filter((s) => values[s] === true);
}

export type FgPermRow = {
  key: keyof FgPerms;
  label: string;
  hint: string;
};

/**
 * Fine-grained permission rows, split by binding — the generated
 * `FG_PERM_CATALOG` is authoritative; every family `pat.createFineGrained`
 * accepts is shown (reserved families carry "Reserved for …" hints).
 */
export const FG_REPO_PERMS: FgPermRow[] = FG_PERM_CATALOG.filter((p) => !p.account_level).map(
  (p) => ({ key: p.family as keyof FgPerms, label: p.label, hint: p.hint }),
);

export const FG_ACCOUNT_PERMS: FgPermRow[] = FG_PERM_CATALOG.filter((p) => p.account_level).map(
  (p) => ({ key: p.family as keyof FgPerms, label: p.label, hint: p.hint }),
);

export type FgPermValues = Record<string, PermChoice>;

/** `perms` payload for `pat.createFineGrained` — `none` entries are dropped. */
export function buildPerms(values: FgPermValues): FgPerms | undefined {
  const out: Record<string, FgPermLevel> = {};
  for (const [key, level] of Object.entries(values)) {
    if (level === "read" || level === "write") out[key] = level;
  }
  return Object.keys(out).length > 0 ? (out as FgPerms) : undefined;
}
