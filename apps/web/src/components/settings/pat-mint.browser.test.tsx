/**
 * Real-Chromium gate for PAT mint DOM races (#42 / #43).
 * Single file so Vite optimizeDeps reload cannot abort a second suite iframe.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PatClassicForm } from "@/components/settings/pat-classic-form";
import { PatFgForm } from "@/components/settings/pat-fg-form";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountComponent,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const createClassicMock = vi.fn();
const createFgMock = vi.fn();
const listMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pat: {
      createClassic: (...args: unknown[]) => createClassicMock(...args),
      createFineGrained: (...args: unknown[]) => createFgMock(...args),
    },
    repo: {
      listMine: (...args: unknown[]) => listMineMock(...args),
    },
  },
}));

beforeEach(() => {
  createClassicMock.mockReset();
  createFgMock.mockReset();
  listMineMock.mockReset();
  createClassicMock.mockResolvedValue({
    ok: true,
    data: { token: "oxidean_pat_test", item: {} },
  });
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

describe("PAT mint browser DOM races", () => {
  it("classic: toggling scope checkboxes does not throw insertBefore / overlay", async () => {
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

  it("fine-grained: toggling repo access and repo checkboxes does not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountComponent(PatFgForm, { onCreated: () => {} });

      const form = document.querySelector('[data-testid="pat-fg-form"]');
      if (!form) {
        throw new Error(`pat-fg-form not mounted. ${debugBody()}`);
      }

      const deadline = Date.now() + 10_000;
      while (!document.querySelector('[data-testid="fg-repo-item-r1"]') && Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 50));
      }
      if (!document.querySelector('[data-testid="fg-repo-item-r1"]')) {
        throw new Error(`fg-repo-item-r1 never appeared. ${debugBody()}`);
      }

      await clickTestId("fg-repo-access-all");
      await clickTestId("fg-repo-access-selected");
      await clickTestId("fg-repo-item-r1");
      await clickTestId("fg-repo-item-r2");

      expect(document.querySelector('[data-testid="pat-fg-summary"]')).toBeTruthy();
      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
