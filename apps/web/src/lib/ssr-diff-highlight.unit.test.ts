import { describe, expect, it } from "vitest";
import { DIFF_LARGE_PATCH_CHARS, SSR_DIFF_HIGHLIGHT_MAX_FILES } from "./diff-budget";
import { ssrHighlightDiffFiles } from "./ssr-diff-highlight";

function smallPatch(marker: string): string {
  return ["@@ -1 +1 @@", "-old", `+${marker}`].join("\n");
}

describe("ssrHighlightDiffFiles", { timeout: 60_000 }, () => {
  it("caps eager highlighting at the first few small files (issue #110)", async () => {
    const files = [
      { path: "a.ts", patch: smallPatch("a") },
      { path: "b.ts", patch: smallPatch("b") },
      { path: "c.ts", patch: smallPatch("c") },
      { path: "d.ts", patch: smallPatch("d") },
      { path: "e.ts", patch: smallPatch("e") },
    ];

    const res = await ssrHighlightDiffFiles(files);

    expect(Object.keys(res.byPath).sort()).toEqual(
      Array.from({ length: SSR_DIFF_HIGHLIGHT_MAX_FILES }, (_, i) => `${"abcde"[i]}.ts`).sort(),
    );
    expect(res.byPath["d.ts"]).toBeUndefined();
    expect(res.byPath["e.ts"]).toBeUndefined();
  });

  it("skips patches above the large-diff gate and empty patches", async () => {
    const bigPatch = `@@ -0,0 +1,2000 @@\n${Array.from(
      { length: 2000 },
      (_, i) => `+line ${i} padding padding`,
    ).join("\n")}`;
    expect(bigPatch.length).toBeGreaterThan(DIFF_LARGE_PATCH_CHARS);

    const res = await ssrHighlightDiffFiles([
      { path: "big.ts", patch: bigPatch },
      { path: "empty.ts", patch: "" },
      { path: "small.ts", patch: smallPatch("x") },
    ]);

    expect(Object.keys(res.byPath)).toEqual(["small.ts"]);
  });

  it("returns empty for no files or a non-positive cap", async () => {
    expect(await ssrHighlightDiffFiles([])).toEqual({ byPath: {} });
    expect(await ssrHighlightDiffFiles([{ path: "a.ts", patch: smallPatch("a") }], 0)).toEqual({
      byPath: {},
    });
  });
});
