/**
 * Real-Chromium gate for classic PAT scope checkbox DOM races (#42 / #43).
 * happy-dom does not throw the insertBefore path for Base UI Checkbox.Indicator.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PatClassicForm } from "@/components/settings/pat-classic-form";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const createClassicMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pat: {
      createClassic: (...args: unknown[]) => createClassicMock(...args),
    },
  },
}));

beforeEach(() => {
  createClassicMock.mockReset();
  createClassicMock.mockResolvedValue({
    ok: true,
    data: { token: "oxidean_pat_test", item: {} },
  });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("PatClassicForm browser DOM races", () => {
  it("toggling scope checkboxes does not throw insertBefore / overlay", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(PatClassicForm, { onCreated: () => {} });

      const form = document.querySelector('[data-testid="pat-classic-form"]');
      if (!form) {
        throw new Error(`pat-classic-form not mounted. ${debugBody()}`);
      }

      await clickTestId("scope-repo");
      await clickTestId("scope-package-read");
      await clickTestId("scope-package-write");
      await clickTestId("scope-repo");

      expect(document.querySelector('[data-testid="pat-classic-summary"]')).toBeTruthy();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
