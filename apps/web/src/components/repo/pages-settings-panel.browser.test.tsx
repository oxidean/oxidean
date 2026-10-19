/**
 * Chromium gate for the Pages settings panel — the form swaps legacy/workflow
 * source controls and mounts an unpublish dialog inside nested `@if` branches,
 * which happy-dom cannot race-check.
 */
import { afterEach, beforeEach, describe, it, vi } from "vitest";
import { PagesSettingsPanel } from "@/components/repo/pages-settings-panel";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const pagesGetMock = vi.fn();
const buildsListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    pages: {
      get: (...args: unknown[]) => pagesGetMock(...args),
      enable: vi.fn(),
      update: vi.fn(),
      disable: vi.fn().mockResolvedValue({ ok: true, data: {} }),
      builds: {
        list: (...args: unknown[]) => buildsListMock(...args),
        get: vi.fn(),
        request: vi.fn(),
      },
      deploys: { create: vi.fn() },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

const SITE = {
  status: "built",
  html_url: "http://localhost/pages/ada/hello/",
  build_type: "legacy",
  source: { branch: "main", path: "/" },
  cname: "",
  https_enforced: true,
  updated_at: "2026-01-01T00:00:00Z",
};

beforeEach(() => {
  pagesGetMock.mockReset();
  pagesGetMock.mockResolvedValue({ ok: true, data: SITE });
  buildsListMock.mockReset();
  buildsListMock.mockResolvedValue({ ok: true, data: { builds: [] } });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

describe("PagesSettingsPanel browser DOM races", () => {
  it("unpublish dialog open/close does not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(PagesSettingsPanel, {
        owner: "ada",
        name: "hello",
        can_admin: true,
      });

      await waitForTestId("pages-settings-panel");
      await waitForTestId("pages-site-url");
      await waitForTestId("pages-disable");

      await clickTestId("pages-disable");
      await waitForTestId("pages-disable-confirm");
      await clickTestId("pages-disable-confirm");
      await clickTestId("pages-disable");
      await waitForTestId("pages-disable-confirm");

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
