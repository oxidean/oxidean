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
  });

  test("no rolling ox-dist feed — the tag release carries the matrix", () => {
    expect(yml).not.toContain("releases/download/ox-dist");
    expect(yml).not.toContain("gh release upload ox-dist");
    expect(yml).not.toContain("gh release create ox-dist");
    expect(yml).not.toContain("publish-dist");
    // latest.json + SOURCE_COMMIT ship as assets on the tag's own release.
    expect(yml).toContain('releases/download/${{ github.ref_name }}');
    const release = yml.slice(yml.indexOf("  release:"));
    expect(release).toContain("assets/SOURCE_COMMIT");
    expect(release).toContain("gh release create");
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

describe("release-beta.yml channel lane", () => {
  const yml = wf("release-beta.yml");

  test("fires on every push to main — main is the beta channel", () => {
    expect(yml).toContain("push:");
    expect(yml).toContain("branches: [main]");
    expect(yml).toContain("concurrency:");
  });

  test("tag-only cut: plan + release-tags on the beta channel, never bump/PR", () => {
    expect(yml).toContain("cli.ts plan --channel beta --json");
    expect(yml).toContain("cli.ts release-tags --channel beta");
    expect(yml).not.toContain("cli.ts bump");
    expect(yml).not.toContain("gh pr create");
  });

  test("tag pushes ride RELEASE_TOKEN so downstream lanes fire", () => {
    expect(yml).toContain("secrets.RELEASE_TOKEN");
    expect(yml).not.toContain("github.token");
  });

  test("fork-gated and pins the pushed SHA", () => {
    expect(yml).toContain("github.repository == 'oxidean/oxidean'");
    expect(yml).toContain("ref: ${{ github.sha }}");
  });
});

describe("production-deploy.yml stable cut", () => {
  const yml = wf("production-deploy.yml");

  test("promote creates the stable release (changelog + bump + tags)", () => {
    expect(yml).toContain("cli.ts plan --channel stable");
    expect(yml).toContain("cli.ts changelog --plan");
    expect(yml).toContain("cli.ts bump --plan");
    expect(yml).toContain("cli.ts release-tags --plan");
    expect(yml).toContain("chore(release):");
  });

  test("first cut on a tagless repo is the 0.1.0 baseline (--all)", () => {
    expect(yml).toContain("release-tags --all");
  });

  test("cut writes via RELEASE_TOKEN and deploys the bump commit", () => {
    expect(yml).toContain("secrets.RELEASE_TOKEN");
    expect(yml).toContain("contents: write");
    expect(yml).toContain("deploy_sha");
  });

  test("fresh ox-v* tag waits for its asset matrix before deploys", () => {
    expect(yml).toContain("ox-cli");
    expect(yml).toMatch(/wait.*ox|ox.*wait/i);
  });
});
