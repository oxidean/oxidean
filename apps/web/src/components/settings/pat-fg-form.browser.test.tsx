/**
 * Real-Chromium gate for fine-grained PAT mint DOM races (#42 / #43).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PatFgForm } from "@/components/settings/pat-fg-form";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const createFgMock = vi.fn();
const listMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pat: {
      createFineGrained: (...args: unknown[]) => createFgMock(...args),
    },
    repo: {
      listMine: (...args: unknown[]) => listMineMock(...args),
    },
  },
}));

beforeEach(() => {
  createFgMock.mockReset();
  listMineMock.mockReset();
  createFgMock.mockResolvedValue({
    ok: true,
    data: { token: "oxidean_fg_test", item: {} },
  });
  listMineMock.mockResolvedValue({
    ok: true,
    data: {
      repos: [
        {
          id: "r1",
          name: "hello",
          owner_username: "ada",
          visibility: "private",
        },
        {
          id: "r2",
          name: "world",
          owner_username: "ada",
          visibility: "public",
        },
      ],
    },
  });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("PatFgForm browser DOM races", () => {
  it("toggling repo access and repo checkboxes does not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(PatFgForm, { onCreated: () => {} });

      const form = document.querySelector('[data-testid="pat-fg-form"]');
      if (!form) {
        throw new Error(`pat-fg-form not mounted. ${debugBody()}`);
      }

      const deadline = Date.now() + 10_000;
      while (!document.querySelector('[data-testid="fg-repo-r1"]') && Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 50));
      }
      if (!document.querySelector('[data-testid="fg-repo-r1"]')) {
        throw new Error(`fg-repo-r1 never appeared. ${debugBody()}`);
      }

      await clickTestId("fg-repo-access-all");
      await clickTestId("fg-repo-access-selected");
      await clickTestId("fg-repo-r1");
      await clickTestId("fg-repo-r2");

      expect(document.querySelector('[data-testid="pat-fg-summary"]')).toBeTruthy();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
