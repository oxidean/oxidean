/**
 * styles.css layout-shift guards — the vertical scrollbar gutter is reserved
 * on every page and `.oxidean-main` keeps the footer below the initial
 * viewport fold, so navigating between short and tall pages never shifts
 * chrome horizontally or pops the footer into view.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("styles.css layout-shift guards", () => {
  it("reserves the scrollbar gutter and keeps the footer below the fold", () => {
    const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    expect(css).toMatch(/scrollbar-gutter:\s*stable/);
    expect(css).toMatch(/overflow-y:\s*scroll/);
    expect(css).toMatch(/\.oxidean-main\s*\{[^}]*min-height:\s*100vh/s);
  });
});
