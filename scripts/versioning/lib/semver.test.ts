import { describe, expect, test } from "bun:test";
import {
  applyBump,
  bumpForCommit,
  compareSemver,
  maxBump,
  parseSemver,
  prereleaseOf,
  withChannel,
} from "./semver.ts";

describe("bumpForCommit — 0.x-aware table (FR3.1/BR3.4)", () => {
  test("breaking at 0.x → minor, at ≥1.0.0 → major", () => {
    expect(bumpForCommit("fix", true, "0.4.2")).toBe("minor");
    expect(bumpForCommit("feat", true, "0.1.0")).toBe("minor");
    expect(bumpForCommit("fix", true, "1.2.3")).toBe("major");
    expect(bumpForCommit("chore", true, "2.0.0")).toBe("major");
  });
  test("feat → minor; fix/perf/revert → patch; housekeeping → none", () => {
    expect(bumpForCommit("feat", false, "0.1.0")).toBe("minor");
    expect(bumpForCommit("fix", false, "0.1.0")).toBe("patch");
    expect(bumpForCommit("perf", false, "1.0.0")).toBe("patch");
    expect(bumpForCommit("revert", false, "1.0.0")).toBe("patch");
    for (const t of ["chore", "docs", "style", "refactor", "test", "build", "ci"]) {
      expect(bumpForCommit(t, false, "0.1.0")).toBe("none");
    }
  });
  test("grammar-invalid commit (no type) → patch evidence", () => {
    expect(bumpForCommit(undefined, false, "0.1.0")).toBe("patch");
  });
});

describe("applyBump", () => {
  test("patch/minor/major arithmetic", () => {
    expect(applyBump("0.1.0", "patch")).toBe("0.1.1");
    expect(applyBump("0.1.0", "minor")).toBe("0.2.0");
    expect(applyBump("1.4.7", "major")).toBe("2.0.0");
    expect(applyBump("0.1.0", "none")).toBe("0.1.0");
  });
  test("existing prerelease is dropped", () => {
    expect(applyBump("0.1.0-alpha.3", "patch")).toBe("0.1.1");
  });
  test("non-semver throws", () => {
    expect(() => applyBump("dev", "patch")).toThrow();
  });
});

describe("withChannel (BR3.6/FR8.3)", () => {
  test("stable → bare semver", () => {
    expect(withChannel("0.2.0", "stable", 0)).toBe("0.2.0");
  });
  test("alpha/beta → -<channel>.N starting at 1", () => {
    expect(withChannel("0.2.0", "alpha", 0)).toBe("0.2.0-alpha.1");
    expect(withChannel("0.2.0", "beta", 2)).toBe("0.2.0-beta.3");
  });
  test("nightly → -nightly.YYYYMMDDN", () => {
    expect(withChannel("0.2.0", "nightly", 0)).toMatch(/^0\.2\.0-nightly\.\d{9}$/);
  });
});

describe("compareSemver / helpers", () => {
  test("orders versions incl. prerelease < release", () => {
    expect(compareSemver("0.1.0", "0.2.0")).toBe(-1);
    expect(compareSemver("0.2.0-alpha.1", "0.2.0")).toBe(-1);
    expect(compareSemver("0.2.0-alpha.1", "0.2.0-alpha.2")).toBe(-1);
    expect(compareSemver("1.0.0", "1.0.0")).toBe(0);
  });
  test("parseSemver / prereleaseOf / maxBump", () => {
    expect(parseSemver("0.1.0-alpha.2")).toEqual({ major: 0, minor: 1, patch: 0, pre: "alpha.2" });
    expect(prereleaseOf("0.1.0-beta.1")).toBe("beta.1");
    expect(prereleaseOf("0.1.0")).toBeNull();
    expect(maxBump("patch", "minor")).toBe("minor");
    expect(maxBump("none", "patch")).toBe("patch");
  });
});
