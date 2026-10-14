import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { loadRegistry } from "./lib/registry";

/**
 * Docs-shape guard for U7 (BR7.1–BR7.3): the contributor-guidance surfaces
 * must exist, cover the required topics, and reference — never duplicate —
 * the machine-owned truth in versioning.toml.
 */
const ROOT = join(import.meta.dir, "../..");
const read = (rel: string) => readFileSync(join(ROOT, rel), "utf-8");

describe("docs/VERSIONING.md (canonical reference)", () => {
  const doc = read("docs/VERSIONING.md");

  test("exists and is substantive", () => {
    expect(doc.length).toBeGreaterThan(4000);
  });

  test("covers the BR7.1 topic enum", () => {
    const anchors = [
      "independent", // independent component versions
      "0.1.0", // baseline
      "type(", // commit grammar
      "BREAKING CHANGE:", // breaking markers
      "allowed_types", // type authority
      "Exemptions", // exempt patterns
      "make setup", // hook install
      "--no-verify", // hook bypass
      "CHANGELOG.md", // per-component changelogs
      "WIRE-CHANGELOG.md", // wire changelog
      "release-plan", // release mechanics
      "release-tags",
      "RELEASE_TOKEN", // maintainer secret (BR7.5)
      "nightly", // prerelease lanes
      "chore(release):", // reserved convention
    ];
    for (const a of anchors) expect(doc).toContain(a);
  });

  test("names versioning.toml as the scope authority (BR7.3)", () => {
    const flat = doc.replace(/\s+/g, " ");
    expect(flat).toContain("versioning.toml` is authoritative");
    expect(flat.toLowerCase()).toContain("illustrative only");
  });

  test("does not duplicate the full scope table (BR7.3)", () => {
    // An illustrative handful is fine; listing every registered alias in
    // table form would be a drift copy.
    const loaded = loadRegistry(ROOT);
    expect(loaded.ok).toBe(true);
    const allAliases = loaded.ok ? loaded.config.entries.flatMap((e) => e.scopes) : [];
    const tableish = allAliases.filter((s) => doc.includes(`\`${s}\``));
    expect(allAliases.length).toBeGreaterThan(10); // sanity: real registry
    expect(tableish.length).toBeLessThanOrEqual(8); // illustrative subset only
  });

  test("GITHUB_TOKEN prohibition is named for tag pushes (BR7.5)", () => {
    expect(doc).toContain("GITHUB_TOKEN");
    expect(doc).toMatch(/GITHUB_TOKEN.{0,80}(does not|prohibit|never)/i);
  });
});

describe("entry points link the convention", () => {
  test("CONTRIBUTING.md links VERSIONING.md", () => {
    const c = read("CONTRIBUTING.md");
    expect(c).toContain("docs/VERSIONING.md");
    expect(c).toContain("type(optional-scope): subject");
  });

  test("AGENTS.md names the hook and the CI gate", () => {
    const a = read("AGENTS.md");
    expect(a).toContain("docs/VERSIONING.md");
    expect(a).toContain("commit-msg");
    expect(a).toContain("commit-lint");
    expect(a).toContain("semver-conventions");
  });

  test("DEVELOPMENT.md names the convention and gates", () => {
    const d = read("docs/DEVELOPMENT.md");
    expect(d).toContain("VERSIONING.md");
    expect(d).toContain("commit-check");
    expect(d).toContain("commit-lint");
  });

  test("TESTING.md covers the versioning test surface", () => {
    const t = read("docs/TESTING.md");
    expect(t).toContain("scripts/versioning");
    expect(t).toContain("version-check");
    expect(t).toContain("commit-lint");
  });
});

describe("agent surfaces", () => {
  test("semver-conventions skill exists with valid frontmatter", () => {
    const s = read(".agents/skills/semver-conventions/SKILL.md");
    expect(s).toMatch(/^---\nname: semver-conventions\ndescription:/);
  });

  test("cursor rules point at the convention", () => {
    const r = read(".cursor/rules/oxidean-core.mdc");
    expect(r).toContain("type(scope): subject");
    expect(r).toContain("semver-conventions");
  });
});
