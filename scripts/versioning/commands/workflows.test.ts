import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * Workflow-shape assertions for the C4 trigger matrix and C5 latest.json
 * gating — the workflows can't run locally, so the contract is pinned as
 * text/structure checks on the YAML.
 */
const ROOT = join(import.meta.dir, "../../..");
const wf = (name: string) =>
  readFileSync(join(ROOT, ".github/workflows", name), "utf-8");

describe("ox-cli.yml retarget", () => {
  const yml = wf("ox-cli.yml");

  test("stable lane triggers on ox-v* tags, not generic v*", () => {
    expect(yml).toContain('tags: ["ox-v*.*.*"]');
    expect(yml).not.toContain('tags: ["v*.*.*"]');
    expect(yml).not.toContain("refs/tags/v'");
    expect(yml).toContain("refs/tags/ox-v");
  });

  test("oxidean-core is in the path filters (FR5.1)", () => {
    expect(yml).toContain("crates/oxidean-core/**");
  });

  test("package version strips the ox-v prefix", () => {
    expect(yml).toContain('GITHUB_REF_NAME#ox-v');
    expect(yml).not.toContain("GITHUB_REF_NAME#v}");
  });

  test("latest.json is written on stable cuts only, never on prereleases", () => {
    expect(yml).toContain("write latest.json (stable cuts only)");
    expect(yml).toContain('"$version" == *-*');
    expect(yml).toContain("schema_version: 1");
    expect(yml).toContain("gh release upload ox-dist assets/latest.json");
  });

  test("rolling ox-dist refresh does not write latest.json", () => {
    const publishDist = yml.slice(yml.indexOf("publish-dist:"), yml.indexOf("release:"));
    expect(publishDist).not.toContain("latest.json");
  });
});

describe("publish-runner.yml retarget", () => {
  const yml = wf("publish-runner.yml");

  test("triggers on the oxidean-runner-v prefix only", () => {
    expect(yml).toContain('tags: ["oxidean-runner-v*.*.*"]');
    expect(yml).not.toContain('tags: ["v*.*.*"]');
  });

  test("component prefix is stripped for the image tag", () => {
    expect(yml).toContain("GITHUB_REF_NAME#oxidean-runner-v");
  });
});

describe("release.yml generic lane", () => {
  const yml = wf("release.yml");

  test("matches every component tag; ox owns its own lane", () => {
    expect(yml).toContain('tags: ["*-v*.*.*"]');
    expect(yml).toContain("!startsWith(github.ref_name, 'ox-v')");
  });

  test("prerelease tags mark the Release prerelease", () => {
    expect(yml).toContain("--prerelease");
    expect(yml).toMatch(/\^?\[0-9\].*-/) // semver `-` gate
  });

  test("dispatch lane takes {component, channel} and never GITHUB_TOKEN-pushes tags", () => {
    expect(yml).toContain("component:");
    expect(yml).toContain("channel:");
    expect(yml).toContain("secrets.RELEASE_TOKEN");
  });
});

describe("release-pr.yml automated lane", () => {
  const yml = wf("release-pr.yml");

  test("runs plan/changelog/bump and opens a chore(release) PR", () => {
    expect(yml).toContain("cli.ts plan");
    expect(yml).toContain("cli.ts changelog");
    expect(yml).toContain("cli.ts bump");
    expect(yml).toContain("chore(release):");
    expect(yml).toContain("release/");
    expect(yml).toContain("gh pr create");
  });
});
