import { describe, expect, it } from "vitest";
import { parseUnifiedDiffLines } from "./parse-unified-diff";
import {
  DIFF_HIGHLIGHT_SOFT_MAX_CHARS,
  escapeHtml,
  highlightDiffLines,
  highlightedDiffRowsEqual,
  plainHighlightedDiffLines,
  splitDiffLinePrefix,
  tokensToInlineHtml,
} from "./highlight-diff";

describe("splitDiffLinePrefix", () => {
  it("splits markers from code lines", () => {
    expect(splitDiffLinePrefix("add", "+const x = 1")).toEqual({
      prefix: "+",
      content: "const x = 1",
    });
    expect(splitDiffLinePrefix("del", "-old")).toEqual({
      prefix: "-",
      content: "old",
    });
    expect(splitDiffLinePrefix("ctx", " keep")).toEqual({
      prefix: " ",
      content: "keep",
    });
    expect(splitDiffLinePrefix("ctx", "")).toEqual({ prefix: "", content: "" });
  });

  it("leaves meta/hunk as full content", () => {
    expect(splitDiffLinePrefix("hunk", "@@ -1 +1 @@")).toEqual({
      prefix: "",
      content: "@@ -1 +1 @@",
    });
  });
});

describe("plainHighlightedDiffLines", () => {
  it("escapes content and preserves prefixes", () => {
    const lines = parseUnifiedDiffLines("+<script>\n keep\n");
    const rows = plainHighlightedDiffLines(lines);
    expect(rows[0]).toMatchObject({
      kind: "add",
      prefix: "+",
      contentHtml: "&lt;script&gt;",
    });
    expect(rows[1]).toMatchObject({
      kind: "ctx",
      prefix: " ",
      contentHtml: "keep",
    });
  });
});

describe("tokensToInlineHtml", () => {
  it("emits light-dark() pairs for dual-theme tokens and escapes content", () => {
    const html = tokensToInlineHtml([
      {
        content: "<b>",
        offset: 0,
        variants: {
          light: { color: "#ff0000", fontStyle: 2 },
          dark: { color: "#00ff00", fontStyle: 2 },
        },
      },
      { content: "x", offset: 3, variants: { light: {}, dark: {} } },
    ]);
    expect(html).toBe(
      '<span style="color:light-dark(#ff0000,#00ff00);font-weight:bold">&lt;b&gt;</span>x',
    );
  });

  it("falls back to the single available variant color", () => {
    const html = tokensToInlineHtml([
      { content: "a", offset: 0, variants: { light: { color: "#123456" } } },
    ]);
    expect(html).toBe('<span style="color:#123456">a</span>');
  });
});

describe("highlightDiffLines", { timeout: 60_000 }, () => {
  it("highlights typescript add/del payloads without wrapping the marker", async () => {
    const patch = ["@@ -1,2 +1,2 @@", " keep", "-const a = 1;", "+const b = 2;"].join("\n");
    const lines = parseUnifiedDiffLines(patch);
    const rows = await highlightDiffLines(lines, {
      path: "src/util.ts",
    });

    const del = rows.find((r) => r.kind === "del");
    const add = rows.find((r) => r.kind === "add");
    expect(del?.prefix).toBe("-");
    expect(add?.prefix).toBe("+");
    expect(del?.contentHtml).toMatch(/style="color:light-dark\(/);
    expect(add?.contentHtml).toMatch(/style="color:light-dark\(/);
    expect(del?.contentHtml).toMatch(/const/);
    expect(add?.contentHtml).not.toMatch(/^\+/);
    // Marker stays outside highlighted HTML.
    expect(del?.contentHtml.startsWith("-")).toBe(false);
  });

  it("leaves plaintext paths escaped without token spans", async () => {
    const lines = parseUnifiedDiffLines("+hello world\n");
    const rows = await highlightDiffLines(lines, {
      path: "notes.txt",
    });
    expect(rows[0]?.contentHtml).toBe("hello world");
    expect(rows[0]?.contentHtml).not.toMatch(/style=/);
  });

  it("skips Shiki when reconstructed sides exceed the soft cap", async () => {
    const big = "x".repeat(DIFF_HIGHLIGHT_SOFT_MAX_CHARS / 2 + 10);
    const lines = parseUnifiedDiffLines(`+${big}\n-${big}\n`);
    const rows = await highlightDiffLines(lines, {
      path: "big.ts",
    });
    expect(rows[0]?.contentHtml).toBe(escapeHtml(big));
    expect(rows[0]?.contentHtml).not.toMatch(/style=/);
  });
});

describe("highlightedDiffRowsEqual", () => {
  it("compares contentHtml deeply", () => {
    const a = plainHighlightedDiffLines(parseUnifiedDiffLines("+a\n"));
    const b = plainHighlightedDiffLines(parseUnifiedDiffLines("+a\n"));
    const c = plainHighlightedDiffLines(parseUnifiedDiffLines("+b\n"));
    expect(highlightedDiffRowsEqual(a, b)).toBe(true);
    expect(highlightedDiffRowsEqual(a, c)).toBe(false);
  });
});
