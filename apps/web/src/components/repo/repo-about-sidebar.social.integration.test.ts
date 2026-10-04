import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));

describe("repo about sidebar social links", () => {
  it("links Watching and Forks for all viewers; Stars only for Write+", () => {
    const src = readFileSync(join(dir, "repo-about-sidebar.tsrx"), "utf8");
    expect(src).toMatch(/base \+ "\/watchers"/);
    expect(src).toMatch(/base \+ "\/forks"/);
    expect(src).toMatch(/base \+ "\/stargazers"/);
    expect(src).toMatch(/canWrite/);
    // Create-fork confirm stays at /fork via chrome; About count goes to /forks list.
    expect(src).not.toMatch(/base \+ "\/fork"/);
  });

  it("ships meta list + Languages section (not i18n)", () => {
    const src = readFileSync(join(dir, "repo-about-sidebar.tsrx"), "utf8");
    expect(src).toMatch(/Settings/);
    expect(src).toMatch(/BookOpen/);
    expect(src).toMatch(/Scale/);
    expect(src).toMatch(/\bUser\b/);
    expect(src).toMatch(/\bActivity\b/);
    expect(src).toMatch(/#readme/);
    expect(src).toMatch(/license\?/);
    expect(src).toMatch(/contributingPath/);
    expect(src).toMatch(/showActivity/);
    expect(src).toMatch(/activityHref/);
    expect(src).not.toMatch(/commitsHref/);
    expect(src).toMatch(/repo-about-languages/);
    expect(src).toMatch(/Languages/);
    expect(src).toMatch(/RepoLanguageStat/);
    // Stars/watching/forks share the flat meta-row style (not a button strip).
    expect(src).toMatch(/META_ROW/);
    expect(src).not.toMatch(/i18n|locale|translation/i);
    // No dead rows for features without product surfaces.
    expect(src).not.toMatch(/Custom properties|Audit log|Report repository/);
    // Homepage links must go through safeExternalHttpUrl (no raw javascript: href).
    expect(src).toMatch(/safeExternalHttpUrl/);
  });

  it("links language legend rows to repo search; Other stays unlinked (issue #59)", () => {
    const src = readFileSync(join(dir, "repo-about-sidebar.tsrx"), "utf8");
    // language:<Name> deep-links into the repo code search.
    expect(src).toMatch(/\/search\?q=/);
    expect(src).toMatch(/encodeURIComponent\("language:" \+ lang\.name\)/);
    // "Other" is a rollup bucket — not a real language qualifier.
    expect(src).toMatch(/lang\.name === "Other"/);
  });
});

describe("empty + populated code home About", () => {
  it("passes showActivity on empty and non-empty code home", () => {
    const index = readFileSync(join(dir, "../../routes/$owner.$repo.index.tsrx"), "utf8");
    const activityTrue = index.match(/showActivity=\{true\}/g) ?? [];
    expect(activityTrue.length).toBeGreaterThanOrEqual(2);
  });
});
