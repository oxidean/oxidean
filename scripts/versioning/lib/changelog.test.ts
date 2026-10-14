import { describe, expect, test } from "bun:test";
import {
  renderRootSection,
  renderSection,
  upsertSection,
} from "./changelog.ts";
import type { PlanEntry } from "./plan.ts";

function entry(over: Partial<PlanEntry> = {}): PlanEntry {
  return {
    component: "core",
    fromVersion: "0.1.0",
    toVersion: "0.2.0",
    bump: "minor",
    direct: true,
    propagatedFrom: [],
    commits: [
      { sha: "aaaaaaa1111", type: "feat", breaking: false, subject: "add thing" },
      { sha: "bbbbbbb2222", type: "fix", breaking: false, subject: "repair stuff" },
      { sha: "ccccccc3333", type: "feat", breaking: true, subject: "new api" },
      { sha: "ddddddd4444", type: "chore", breaking: false, subject: "tidy" },
    ],
    ...over,
  };
}

describe("renderSection", () => {
  test("renders the Keep-a-Changelog heading and type buckets", () => {
    const s = renderSection(entry(), "2026-10-15");
    expect(s).toContain("## [0.2.0] - 2026-10-15");
    expect(s).toContain("### Features");
    expect(s).toContain("### Fixes");
    expect(s).toContain("- add thing (aaaaaaa)");
    expect(s).toContain("- new api (ccccccc) **breaking**");
    // chore commits are not changelog evidence
    expect(s).not.toContain("tidy");
    expect(s).not.toContain("### Changed");
  });

  test("propagatedFrom renders a Dependencies bucket", () => {
    const s = renderSection(
      entry({ bump: "patch", commits: [], propagatedFrom: ["core", "db"] }),
      "2026-10-15",
    );
    expect(s).toContain("### Dependencies");
    expect(s).toContain("`core`");
    expect(s).toContain("`db`");
  });

  test("empty evidence falls back to a Changed maintenance note", () => {
    const s = renderSection(entry({ commits: [] }), "2026-10-15");
    expect(s).toContain("### Changed");
    expect(s).toContain("Maintenance release");
  });
});

describe("upsertSection", () => {
  test("empty file gets the standard header", () => {
    const out = upsertSection("", "0.2.0", renderSection(entry(), "2026-10-15"));
    expect(out).toContain("# Changelog");
    expect(out).toContain("## [0.2.0] - 2026-10-15");
  });

  test("inserts above an existing older section", () => {
    const existing = "# Changelog\n\n## [0.1.0] - 2026-10-01\n\n- old\n";
    const out = upsertSection(existing, "0.2.0", renderSection(entry(), "2026-10-15"));
    const iNew = out.indexOf("## [0.2.0]");
    const iOld = out.indexOf("## [0.1.0]");
    expect(iNew).toBeGreaterThan(-1);
    expect(iNew).toBeLessThan(iOld);
    expect(out).toContain("- old");
  });

  test("regenerating a version replaces it in place — never duplicates", () => {
    const first = upsertSection("", "0.2.0", renderSection(entry(), "2026-10-15"));
    const revised = entry({
      commits: [
        { sha: "eeeeeee5555", type: "fix", breaking: false, subject: "different" },
      ],
    });
    const second = upsertSection(first, "0.2.0", renderSection(revised, "2026-10-16"));
    expect(second.match(/## \[0\.2\.0\]/g)).toHaveLength(1);
    expect(second).toContain("## [0.2.0] - 2026-10-16");
    expect(second).toContain("different");
    expect(second).not.toContain("add thing");
    // idempotent: a third identical upsert is a no-op
    expect(upsertSection(second, "0.2.0", renderSection(revised, "2026-10-16"))).toBe(second);
  });

  test("replacing a middle section preserves neighbours", () => {
    let text = "# Changelog\n";
    text = upsertSection(text, "0.3.0", "## [0.3.0] - 2026-10-20\n\n- three\n");
    text = upsertSection(text, "0.2.0", "## [0.2.0] - 2026-10-15\n\n- two\n");
    text = upsertSection(text, "0.1.0", "## [0.1.0] - 2026-10-01\n\n- one\n");
    // replace the middle one
    text = upsertSection(text, "0.2.0", "## [0.2.0] - 2026-10-16\n\n- two-revised\n");
    expect(text).toContain("- three");
    expect(text).toContain("- two-revised");
    expect(text).toContain("- one");
    expect(text.match(/## \[/g)).toHaveLength(3);
  });
});

describe("renderRootSection", () => {
  test("links only bumped components with their tags", () => {
    const s = renderRootSection(
      [
        entry({ component: "core" }),
        entry({ component: "app", toVersion: "0.1.0", bump: "none" }),
      ],
      "2026-10-15",
    );
    expect(s).toContain("## Release 2026-10-15");
    expect(s).toContain("`core` 0.1.0 → 0.2.0");
    expect(s).toContain("core-v0.2.0");
    expect(s).not.toContain("`app`");
  });
});
