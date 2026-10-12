/**
 * Chromium proof for the MCP harness Select beside the snippet block —
 * portal option picks must not race the sibling re-render.
 */
import { afterEach, describe, expect, it } from "vitest";
import { McpHarnessBlocks } from "@/components/settings/mcp-harness-blocks";
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

      // Auth-mode radio swaps the block to the OAuth variant (no header) and
      // mounts the note — sibling @if toggles are the DOM-race surface.
      const oauthRadio = document.querySelector(
        '[data-testid="mcp-auth-oauth"]',
      ) as HTMLElement | null;
      oauthRadio?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      await new Promise((r) => setTimeout(r, 50));
      const swapped = document.querySelector('[data-testid="mcp-snippet-block"]');
      expect(swapped?.getAttribute("data-auth")).toBe("oauth");
      expect(swapped?.getAttribute("data-harness")).toBe("codex");
      expect(swapped?.textContent).not.toContain("bearer_token_env_var");
      expect(document.querySelector('[data-testid="mcp-snippet-note"]')?.textContent).toContain(
        "codex mcp login",
      );

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
