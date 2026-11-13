/**
 * Chromium proof for /settings/experiments — the Switch toggle swaps loading
 * state over a mounted list; pending-key disable + cache write must not race
 * the row re-render.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const meMock = vi.fn();
const listMock = vi.fn();
const setMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: { me: (...args: unknown[]) => meMock(...args) },
    experiments: {
      list: (...args: unknown[]) => listMock(...args),
      set: (...args: unknown[]) => setMock(...args),
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  appToastManager: {
    add: vi.fn(),
    remove: vi.fn(),
    update: vi.fn(),
    close: vi.fn(),
    promise: vi.fn(),
  },
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

import { ExperimentsSettingsPage } from "./experiments.tsrx";

const USER = {
  id: "u1",
  username: "casey",
  display_name: "Casey",
  primary_email: "c@ex.com",
  experiments: ["pr-new-ui"],
};

const EXPERIMENTS = {
  experiments: [
    {
      key: "pr-new-ui",
      title: "New pull request experience",
      description: "GitHub-style tabs and changes view.",
      status: "experimental",
      enabled: true,
      since_version: "0.7.0",
    },
  ],
};

async function settle(ms = 250) {
  await act(async () => {
    await new Promise((r) => setTimeout(r, ms));
  });
}

beforeEach(() => {
  meMock.mockResolvedValue({ ok: true, data: USER });
  listMock.mockResolvedValue({ ok: true, data: EXPERIMENTS });
  setMock.mockResolvedValue({ ok: true, data: { ...USER, experiments: [] } });
});

describe("settings experiments browser DOM races", () => {
  it("toggling a flag swaps row state without insertBefore races", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(ExperimentsSettingsPage, {});
      await settle(400);

      const page = document.querySelector('[data-testid="settings-experiments-page"]');
      if (!page) throw new Error(`experiments page not mounted. ${debugBody()}`);

      const toggle = document.querySelector(
        '[data-testid="experiment-toggle-pr-new-ui"]',
      ) as HTMLElement | null;
      if (!toggle) throw new Error(`toggle missing. ${debugBody()}`);

      await act(async () => {
        toggle.click();
      });
      await settle(300);

      // experiments.set was called to disable; the row re-rendered in place.
      expect(setMock).toHaveBeenCalledWith({ key: "pr-new-ui", enabled: false });
      expect(document.querySelector('[data-testid="experiment-toggle-pr-new-ui"]')).not.toBeNull();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  });
});
