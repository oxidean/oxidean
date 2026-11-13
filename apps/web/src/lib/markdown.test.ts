import { describe, expect, it } from "vitest";
import { isMarkdownPath, renderGfm } from "./markdown";

describe("isMarkdownPath", () => {
  it("detects common markdown extensions and bare README", () => {
    expect(isMarkdownPath("docs/guide.md")).toBe(true);
    expect(isMarkdownPath("README.markdown")).toBe(true);
    expect(isMarkdownPath("README")).toBe(true);
    expect(isMarkdownPath("src/main.rs")).toBe(false);
  });
});

describe("renderGfm", () => {
  it("strips script tags and unsafe HTML", async () => {
    const html = await renderGfm('Hello <script>alert("xss")</script> **world**');
    // Tags/handlers must go; inert text left after strip is not executable XSS.
    expect(html).not.toMatch(/<script/i);
    expect(html).not.toMatch(/\son\w+=/i);
    expect(html).toMatch(/<strong>world<\/strong>/i);
  });

  it("renders GFM tables without raw HTML passthrough", async () => {
    const html = await renderGfm("| a | b |\n| --- | --- |\n| 1 | <img onerror=alert(1) src=x> |");
    expect(html).toMatch(/<table/i);
    expect(html).not.toMatch(/onerror/i);
  });

  it("renders remote and linked images (badges) with lazy-loading attrs", async () => {
    const html = await renderGfm(
      "[![badge](https://img.shields.io/badge/x-y-green)](https://example.com)\n\n![shot](https://example.com/i.png)",
    );
    expect(html).toMatch(/<a href="https:\/\/example\.com"><img /);
    expect(html).toMatch(/src="https:\/\/img\.shields\.io\/badge\/x-y-green"/);
    expect(html).toMatch(/loading="lazy"/);
    expect(html).toMatch(/decoding="async"/);
  });

  it("still strips unsafe image sources", async () => {
    const html = await renderGfm("![x](javascript:alert(1)) ![d](data:image/png;base64,AA)");
    expect(html).not.toMatch(/<img[^>]*src=/i);
  });
});
