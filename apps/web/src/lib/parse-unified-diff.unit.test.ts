import { describe, expect, it } from "vitest";
import { classifyDiffLine, diffPatchStatCounts, parseUnifiedDiffLines } from "./parse-unified-diff";

describe("classifyDiffLine", () => {
  it("tags add/del/ctx/hunk/meta", () => {
    expect(classifyDiffLine("+foo")).toBe("add");
    expect(classifyDiffLine("-bar")).toBe("del");
    expect(classifyDiffLine(" baz")).toBe("ctx");
    expect(classifyDiffLine("@@ -1,2 +3,4 @@")).toBe("hunk");
    expect(classifyDiffLine("diff --git a/x b/x")).toBe("meta");
    expect(classifyDiffLine("--- /dev/null")).toBe("meta");
    expect(classifyDiffLine("+++ b/x")).toBe("meta");
    expect(classifyDiffLine("\\ No newline at end of file")).toBe("note");
  });
});

describe("parseUnifiedDiffLines", () => {
  it("splits a sample added-file patch into styled rows", () => {
    const patch = [
      "diff --git a/README.md b/README.md",
      "new file mode 100644",
      "index 0000000..abc1234",
      "--- /dev/null",
      "+++ b/README.md",
      "@@ -0,0 +1,2 @@",
      "+# Hello",
      "+world",
      "",
    ].join("\n");
    const lines = parseUnifiedDiffLines(patch);
    expect(lines.map((l) => l.kind)).toEqual([
      "meta",
      "meta",
      "meta",
      "meta",
      "meta",
      "hunk",
      "add",
      "add",
    ]);
    expect(lines[6]?.text).toBe("+# Hello");
  });

  it("handles CRLF and mixed change hunks", () => {
    const patch = "@@ -1,3 +1,3 @@\r\n context\r\n-old\r\n+new\r\n keep\r\n";
    const lines = parseUnifiedDiffLines(patch);
    expect(lines.map((l) => l.kind)).toEqual(["hunk", "ctx", "del", "add", "ctx"]);
  });
});

describe("diffPatchStatCounts", () => {
  it("counts +/− lines inside hunk budgets, not file headers", () => {
    const patch = [
      "diff --git a/a.ts b/a.ts",
      "index 111..222 100644",
      "--- a/a.ts",
      "+++ b/a.ts",
      "@@ -1,3 +1,3 @@",
      " keep",
      "-old",
      "+new",
    ].join("\n");
    expect(diffPatchStatCounts(patch)).toEqual([1, 1]);
  });

  it("treats --- inside a hunk's old-line budget as a real deletion", () => {
    const patch = ["@@ -1,2 +1,2 @@", " ctx", "--- markdown hr removed", "+replacement"].join("\n");
    expect(diffPatchStatCounts(patch)).toEqual([1, 1]);
  });

  it("returns zeros for empty and header-only patches", () => {
    expect(diffPatchStatCounts("")).toEqual([0, 0]);
    expect(diffPatchStatCounts("diff --git a/x b/x\nindex 1..2 100644")).toEqual([0, 0]);
  });
});
