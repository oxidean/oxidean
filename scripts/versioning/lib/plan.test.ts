import { beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { parseCommit, type ConventionalCommit } from "./commit.ts";
import { attributeCommit, computePlan, propagate, type RangeCommit } from "./plan.ts";
import type { RegistryConfig, RegistryEntry } from "./registry.ts";
import type { Git } from "./git.ts";

// --- fixtures ---------------------------------------------------------------

const entry = (o: Partial<RegistryEntry> & { id: string }): RegistryEntry => ({
  kind: "crate",
  tag_prefix: `${o.id}-v`,
  changelog_path: `crates/${o.id}/CHANGELOG.md`,
  globs: [`crates/${o.id}/**`],
  scopes: [],
  depends_on: [],
  manifest_path: `crates/${o.id}/Cargo.toml`,
  ...o,
});

const REGISTRY: RegistryConfig = {
  version: 1,
  baseline_commit: "pending",
  allowed_types: ["feat", "fix", "chore", "docs", "style", "refactor", "perf", "test", "build", "ci", "revert"],
  reserved_scopes: ["release"],
  entries: [
    entry({ id: "core", scopes: ["core"] }),
    entry({ id: "db", scopes: ["db"], depends_on: ["core"] }),
    entry({ id: "api", scopes: ["api"], depends_on: ["core", "db", "wire"] }),
    entry({
      id: "wire",
      kind: "contract",
      manifest_path: undefined,
      constant_path: "crates/core/src/lib.rs#WIRE_VERSION",
      globs: [],
      scopes: ["wire"],
    }),
  ],
};

let ROOT: string;
const FAKE_GIT: Git = {
  headSha: () => "f".repeat(40),
  rangeCommits: () => [],
  commitFiles: () => [],
  commitsSince: () => new Set(),
  prefixedTags: () => [],
};

beforeAll(() => {
  ROOT = mkdtempSync(join(tmpdir(), "u3-plan-"));
  for (const id of ["core", "db", "api"]) {
    mkdirSync(join(ROOT, `crates/${id}`), { recursive: true });
    writeFileSync(
      join(ROOT, `crates/${id}/Cargo.toml`),
      `[package]\nname = "${id}"\nversion = "0.1.0"\n`,
    );
  }
  mkdirSync(join(ROOT, "crates/core/src"), { recursive: true });
  writeFileSync(
    join(ROOT, "crates/core/src/lib.rs"),
    'pub const WIRE_VERSION: &str = "0.1.0";\n',
  );
});

function commit(
  sha: string,
  subject: string,
  files: string[],
  body = "",
): RangeCommit {
  const parsed: ConventionalCommit & { sha: string } = {
    ...parseCommit({ subject, body, sha }, REGISTRY),
    sha,
  };
  return { parsed, subject, files };
}

// --- attribution (BR3.2) ----------------------------------------------------

describe("attributeCommit (BR3.2)", () => {
  test("mapped scope attributes even without matching files", () => {
    const c = commit("a".repeat(40), "feat(core): x", []);
    const attrs = attributeCommit({ ...c.parsed, subject: c.subject }, [], REGISTRY.entries, []);
    expect(attrs).toEqual([
      expect.objectContaining({ componentId: "core", source: "scope" }),
    ]);
  });

  test("unmapped scope falls back to globs", () => {
    const c = commit("b".repeat(40), "feat(nope): x", ["crates/db/src/lib.rs"]);
    const attrs = attributeCommit({ ...c.parsed, subject: c.subject }, c.files, REGISTRY.entries, []);
    expect(attrs.map((a) => a.componentId)).toEqual(["db"]);
    expect(attrs[0].source).toBe("glob");
  });

  test("mapped scope unions with globs — cross-cutting commits hit all", () => {
    const c = commit("c".repeat(40), "feat(api): x", ["crates/db/src/lib.rs"]);
    const attrs = attributeCommit({ ...c.parsed, subject: c.subject }, c.files, REGISTRY.entries, []);
    const byId = Object.fromEntries(attrs.map((a) => [a.componentId, a.source]));
    expect(byId).toEqual({ api: "scope", db: "glob" });
  });

  test("multi-entry glob match attributes to all + warns", () => {
    const e1 = entry({ id: "a", globs: ["shared/**"] });
    const e2 = entry({ id: "b", globs: ["shared/**"] });
    const c = commit("d".repeat(40), "feat(x): x", ["shared/f.ts"]);
    const warnings: string[] = [];
    const attrs = attributeCommit({ ...c.parsed, subject: c.subject }, c.files, [e1, e2], warnings);
    expect(attrs.map((a) => a.componentId).sort()).toEqual(["a", "b"]);
    expect(attrs.every((a) => a.ambiguous)).toBe(true);
    expect(warnings.some((w) => w.includes("matched"))).toBe(true);
  });
});

// --- propagation (BR3.3/3.5, FR3.2) -----------------------------------------

describe("propagate (BR3.3)", () => {
  test("dependent closure names origins, transitively", () => {
    // core bumped → db + api depend on core; api also depends on db (closure via two paths).
    const prop = propagate(REGISTRY.entries, new Set(["core"]));
    expect([...prop.get("db")!]).toEqual(["core"]);
    expect([...prop.get("api")!].sort()).toEqual(["core"]);
  });

  test("a propagated dependent propagates onward (transitive)", () => {
    const entries = [
      entry({ id: "a" }),
      entry({ id: "b", depends_on: ["a"] }),
      entry({ id: "c", depends_on: ["b"] }),
    ];
    const prop = propagate(entries, new Set(["a"]));
    expect([...prop.get("b")!]).toEqual(["a"]);
    expect([...prop.get("c")!]).toEqual(["a"]); // origin id, not the intermediary
  });
});

// --- computePlan (FR3.1/3.2, C3) --------------------------------------------

describe("computePlan", () => {
  const opts = { rootDir: "", git: FAKE_GIT, channel: "stable" as const };

  test("feat → minor at 0.x; propagated dep gets patch", () => {
    const plan = computePlan(
      REGISTRY,
      [commit("a".repeat(40), "feat(core): big", ["crates/core/src/lib.rs"])],
      { ...opts, rootDir: ROOT },
    );
    const core = plan.entries.find((e) => e.component === "core")!;
    expect(core.bump).toBe("minor");
    expect(core.toVersion).toBe("0.2.0");
    // db and api depend on core → patch propagation
    const db = plan.entries.find((e) => e.component === "db")!;
    expect(db.bump).toBe("patch");
    expect(db.direct).toBe(false);
    expect(db.propagatedFrom).toEqual(["core"]);
    expect(db.toVersion).toBe("0.1.1");
    expect(plan.entries.find((e) => e.component === "api")!.propagatedFrom).toEqual(["core"]);
  });

  test("chore-only → no entry and no propagation (FR3.2)", () => {
    const plan = computePlan(
      REGISTRY,
      [commit("b".repeat(40), "chore(core): tidy", ["crates/core/x.rs"])],
      { ...opts, rootDir: ROOT },
    );
    expect(plan.entries).toEqual([]);
  });

  test("exempt + reserved commits produce nothing", () => {
    const plan = computePlan(
      REGISTRY,
      [
        commit("c".repeat(40), "Merge branch 'x'", ["crates/core/x.rs"]),
        commit("d".repeat(40), "chore(release): cut", ["crates/db/y.rs"]),
        commit("e".repeat(40), "feat(release): no fallback", ["crates/db/y.rs"]),
      ],
      { ...opts, rootDir: ROOT },
    );
    expect(plan.entries).toEqual([]);
  });

  test("direct + propagated → max(direct, patch)", () => {
    const plan = computePlan(
      REGISTRY,
      [
        commit("f".repeat(40), "feat(core): x", ["crates/core/a.rs"]),
        commit("0".repeat(40), "fix(db): y", ["crates/db/b.rs"]),
      ],
      { ...opts, rootDir: ROOT },
    );
    const db = plan.entries.find((e) => e.component === "db")!;
    expect(db.bump).toBe("patch"); // max(patch, patch)
    expect(db.direct).toBe(true);
    expect(db.propagatedFrom).toEqual(["core"]);
  });

  test("per-component tag bound excludes pre-tag commits (FR3)", () => {
    const before = "1".repeat(40);
    const after = "2".repeat(40);
    const bounds = new Map([["core", new Set([after])]]);
    const plan = computePlan(
      REGISTRY,
      [
        commit(after, "feat(core): new", ["crates/core/n.rs"]),
        commit(before, "feat(core): old", ["crates/core/o.rs"]),
      ],
      { ...opts, rootDir: ROOT, componentBounds: bounds },
    );
    const core = plan.entries.find((e) => e.component === "core")!;
    expect(core.commits.map((c) => c.sha)).toEqual([after]);
  });

  test("channel alpha → -alpha.1 suffix, counting prior tags", () => {
    const git: Git = {
      ...FAKE_GIT,
      prefixedTags: (p) =>
        p === "core-v"
          ? [{ tag: "core-v0.2.0-alpha.1", version: "0.2.0-alpha.1", commit: "x".repeat(40) }]
          : [],
    };
    const plan = computePlan(
      REGISTRY,
      [commit("3".repeat(40), "feat(core): x", ["crates/core/a.rs"])],
      { rootDir: ROOT, git, channel: "alpha" },
    );
    expect(plan.entries.find((e) => e.component === "core")!.toVersion).toBe("0.2.0-alpha.2");
  });

  test("C3 payload shape: sorted entries, schema/version/baseRef", () => {
    const plan = computePlan(
      REGISTRY,
      [commit("4".repeat(40), "fix(core): x", ["crates/core/a.rs"])],
      { ...opts, rootDir: ROOT },
    );
    expect(plan.schema).toBe("release-plan");
    expect(plan.version).toBe(1);
    expect(plan.baseRef).toBe("f".repeat(40));
    const ids = plan.entries.map((e) => e.component);
    expect(ids).toEqual([...ids].sort());
    const core = plan.entries.find((e) => e.component === "core")!;
    expect(core).toHaveProperty("fromVersion", "0.1.0");
    expect(core.commits[0]).toHaveProperty("subject");
  });

  test("grammar-invalid commit still attributes via globs at patch level", () => {
    const plan = computePlan(
      REGISTRY,
      [commit("5".repeat(40), "no grammar here", ["crates/db/x.rs"])],
      { ...opts, rootDir: ROOT },
    );
    const db = plan.entries.find((e) => e.component === "db")!;
    expect(db.bump).toBe("patch");
    expect(db.commits[0].type).toBeUndefined();
    expect(plan.warnings.some((w) => w.includes("grammar-invalid"))).toBe(true);
  });
});
