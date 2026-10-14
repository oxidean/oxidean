import { describe, expect, test } from "bun:test";
import { parseCommit, type CommitMessage } from "./commit.ts";
import type { RegistryConfig } from "./registry.ts";

const registry: Pick<RegistryConfig, "allowed_types" | "reserved_scopes" | "entries"> = {
  allowed_types: ["feat", "fix", "chore", "docs", "style", "refactor", "perf", "test", "build", "ci", "revert"],
  reserved_scopes: ["release"],
  entries: [
    {
      id: "oxidean-core",
      kind: "crate",
      manifest_path: "crates/oxidean-core/Cargo.toml",
      tag_prefix: "oxidean-core-v",
      changelog_path: "crates/oxidean-core/CHANGELOG.md",
      globs: ["crates/oxidean-core/**"],
      scopes: ["core"],
      depends_on: [],
    },
    {
      id: "oxidean-api",
      kind: "crate",
      manifest_path: "crates/oxidean-api/Cargo.toml",
      tag_prefix: "oxidean-api-v",
      changelog_path: "crates/oxidean-api/CHANGELOG.md",
      globs: ["crates/oxidean-api/**"],
      scopes: ["api"],
      depends_on: ["oxidean-core"],
    },
  ],
};

const parse = (subject: string, extra: Partial<CommitMessage> = {}) =>
  parseCommit({ subject, ...extra }, registry);

describe("parseCommit — grammar (BR2.1)", () => {
  test("feat(core): x → mapped, resolved to component", () => {
    const r = parse("feat(core): add thing");
    expect(r.valid).toBe(true);
    expect(r.type).toBe("feat");
    expect(r.scope).toBe("core");
    expect(r.scope_status).toBe("mapped");
    expect(r.resolved).toEqual(["oxidean-core"]);
  });

  test.each([
    ["Feat(core): x", "lowercase"],
    ["bogus(core): x", "unknown type"],
  ])("%s → invalid (%s)", (subject, hint) => {
    const r = parse(subject);
    expect(r.valid).toBe(false);
    expect(r.error).toBeTruthy();
  });

  test("no colon / malformed header → invalid", () => {
    for (const s of ["no colon here", "feat no colon", ""]) {
      const r = parse(s);
      expect(r.valid).toBe(false);
    }
  });

  test("multi-token scope → invalid", () => {
    for (const s of ["feat(a b): x", "feat(a,b): x", "feat( ): x"]) {
      const r = parse(s);
      expect(r.valid).toBe(false);
      expect(r.error).toContain("scope");
    }
  });
});

describe("parseCommit — breaking (BR2.2)", () => {
  test.each([
    ["feat!: drop it", true],
    ["feat(core)!: drop it", true],
    ["feat(core): keep it", false],
  ])("%s → breaking=%s", (subject, expected) => {
    expect(parse(subject).breaking).toBe(expected);
  });

  test.each([
    ["BREAKING CHANGE: removed the thing"],
    ["BREAKING-CHANGE: removed the thing"],
  ])("footer %s → breaking", (footer) => {
    const r = parse("feat(core): x", { body: `some body\n\n${footer}` });
    expect(r.breaking).toBe(true);
  });
});

describe("parseCommit — exemptions (BR2.3)", () => {
  test.each([
    "Merge pull request #1",
    'Revert "feat(core): x"',
    "chore(release): cut 0.1.0",
  ])("%s → exempt, no type needed", (subject) => {
    const r = parse(subject);
    expect(r.valid).toBe(true);
    expect(r.exempt).toBe(true);
    expect(r.type).toBeUndefined();
    expect(r.scope_status).toBe("absent");
    expect(r.resolved).toEqual([]);
    expect(r.breaking).toBe(false);
    expect(r.warnings).toEqual([]);
  });
});

describe("parseCommit — scope states + warnings", () => {
  test("unmapped scope → valid with warning", () => {
    const r = parse("feat(nope): x");
    expect(r.valid).toBe(true);
    expect(r.scope_status).toBe("unmapped");
    expect(r.warnings.some((w) => w.includes("no registry component"))).toBe(true);
  });

  test("reserved scope → reserved status + warning", () => {
    const r = parse("chore(release): notes");
    // chore(release): is exempt — the exemption fires before scope resolution
    expect(r.exempt).toBe(true);
    const r2 = parse("feat(release): odd");
    expect(r2.valid).toBe(true);
    expect(r2.scope_status).toBe("reserved");
  });

  test("no scope → absent", () => {
    const r = parse("feat: x");
    expect(r.scope_status).toBe("absent");
  });

  test("long subject → advisory warning, still valid", () => {
    const r = parse(`feat(core): ${"x".repeat(120)}`);
    expect(r.valid).toBe(true);
    expect(r.warnings.some((w) => w.includes("advisory"))).toBe(true);
  });
});
