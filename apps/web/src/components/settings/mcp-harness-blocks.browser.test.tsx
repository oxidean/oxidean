/**
 * Chromium proof for the MCP harness Select beside the snippet block —
 * portal option picks must not race the sibling re-render.
 */
import { afterEach, describe, expect, it } from "vitest";
import { McpHarnessBlocks } from "@/components/settings/mcp-harness-blocks";
import { getHighlighter } from "@/lib/highlight";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("mcp-harness-blocks browser DOM races", () => {
  it("selecting harnesses swaps the snippet without insertBefore / overlay", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(McpHarnessBlocks, {
        endpoint: "https://forge.example.com/api/mcp",
      });

      const block = document.querySelector('[data-testid="mcp-snippet-block"]');
      if (!block) {
        throw new Error(`mcp-snippet-block not mounted. ${debugBody()}`);
      }

      // Default (first) harness renders the endpoint URL in the snippet.
      expect(block.getAttribute("data-harness")).toBe("claude");
      expect(block.textContent).toContain("https://forge.example.com/api/mcp");

      await pickSelectOptionByTestId("mcp-harness-select", "mcp-harness-option-zed");
      expect(
        document.querySelector('[data-testid="mcp-snippet-block"]')?.getAttribute("data-harness"),
      ).toBe("zed");
      expect(document.querySelector('[data-testid="mcp-snippet-file"]')?.textContent).toContain(
        ".zed/settings.json",
      );

      await pickSelectOptionByTestId("mcp-harness-select", "mcp-harness-option-codex");
      expect(document.querySelector('[data-testid="mcp-snippet-block"]')?.textContent).toContain(
        "mcp_servers.oxidean",
      );

      // Auth-mode toggle flips visibility between the two permanently mounted
      // variant stacks — no <pre> replacement, no mount/unmount, no height or
      // scroll change (the scrolled-to-bottom flicker regression). Warm the
      // highlighter so both variants' markup is styled.
      await getHighlighter();
      // Both variants highlight async on mount — wait for styled markup before
      // capturing nodes, or the pending upgrade would replace the pre and the
      // identity assertion would race it.
      {
        const deadline = Date.now() + 5_000;
        while (Date.now() < deadline) {
          const t = document.querySelector('[data-testid="mcp-variant-token"] pre[data-language]');
          const o = document.querySelector('[data-testid="mcp-variant-oauth"] pre[data-language]');
          if (t && o) break;
          await new Promise((r) => setTimeout(r, 25));
        }
      }
      const tokenPre = document.querySelector('[data-testid="mcp-variant-token"] pre');
      const oauthPre = document.querySelector('[data-testid="mcp-variant-oauth"] pre');
      const heightBefore = document
        .querySelector('[data-testid="mcp-snippet-block"]')
        ?.getBoundingClientRect().height;
      const scrollBefore = window.scrollY;

      const oauthToggle = document.querySelector(
        '[data-testid="mcp-auth-oauth"]',
      ) as HTMLElement | null;
      oauthToggle?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 50));

      const swapped = document.querySelector('[data-testid="mcp-snippet-block"]');
      expect(swapped?.getAttribute("data-auth")).toBe("oauth");
      expect(swapped?.getAttribute("data-harness")).toBe("codex");

      // Regression: the same <pre> nodes survive the toggle — no structural
      // DOM mutation — and the card height + scroll position are unchanged.
      expect(document.querySelector('[data-testid="mcp-variant-token"] pre')).toBe(tokenPre);
      expect(document.querySelector('[data-testid="mcp-variant-oauth"] pre')).toBe(oauthPre);
      expect(swapped?.getBoundingClientRect().height).toBe(heightBefore);
      expect(window.scrollY).toBe(scrollBefore);

      const oauthVariant = document.querySelector('[data-testid="mcp-variant-oauth"]');
      expect(oauthVariant?.classList.contains("invisible")).toBe(false);
      expect(oauthVariant?.textContent).not.toContain("bearer_token_env_var");
      expect(oauthVariant?.querySelector("pre[data-language]")).toBeTruthy();
      const tokenVariant = document.querySelector('[data-testid="mcp-variant-token"]');
      expect(tokenVariant?.classList.contains("invisible")).toBe(true);
      expect(
        document.querySelector('[data-testid="mcp-snippet-note-oauth"]')?.textContent,
      ).toContain("codex mcp login");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
