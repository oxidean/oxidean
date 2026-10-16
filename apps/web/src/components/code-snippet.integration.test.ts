/**
 * CodeSnippet — prop changes on a warm highlighter must render styled markup
 * in the same commit. The Token/OAuth toggle on /settings/ai flickered
 * unstyled → styled because every `code` change painted a plain-text
 * intermediate frame while `highlightCode` awaited.
 */
import { cleanup, render, waitFor } from "@octanejs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { CodeSnippet } from "@/components/code-snippet";
import { ensureLanguage } from "@/lib/highlight";

afterEach(cleanup);

describe("CodeSnippet", () => {
  it("code-prop swaps stay styled once the highlighter is warm", async () => {
    // Cold engine/grammar legitimately uses the plain → async path; warm it.
    await ensureLanguage("json");

    const { container, rerender } = render(CodeSnippet as never, {
      props: { code: '{"a":1}', path: "config.json" },
    });
    await waitFor(() => {
      expect(container.querySelector("pre[data-language]")).toBeTruthy();
    });

    rerender({ props: { code: '{"b":2}', path: "config.json" } });

    // Synchronously after the rerender — plainHtml output has no
    // data-language marker, so its absence would prove an unstyled frame.
    const pre = container.querySelector("pre[data-language]");
    expect(pre).toBeTruthy();
    expect(pre?.textContent).toContain('"b":2');
  });

  it("seeds styled markup on mount once the highlighter is warm", async () => {
    await ensureLanguage("json");
    const { container } = render(CodeSnippet as never, {
      props: { code: '{"x":1}', path: "config.json" },
    });
    // No waitFor — first paint is already styled, not a plain-text frame.
    const pre = container.querySelector("pre[data-language]");
    expect(pre).toBeTruthy();
    expect(pre?.textContent).toContain('"x":1');
  });
});
