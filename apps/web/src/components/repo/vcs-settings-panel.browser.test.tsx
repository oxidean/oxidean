/**
 * Chromium gate for the VCS protocol flag toggle: Base UI Checkbox clicks +
 * form submit must not throw Octane insertBefore races (the Checkbox
 * indicator `keepMounted` precedent) and the save path drives
 * `vcs.repoEnable`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "octane";
import { VcsSettingsPanel } from "@/components/repo/vcs-settings-panel";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";

const statusMock = vi.fn();
const enableMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    vcs: {
      status: (...args: unknown[]) => statusMock(...args),
      repoEnable: (...args: unknown[]) => enableMock(...args),
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

const STATUS_OFF = {
  enabled: false,
  heads: [],
  changeset_count: 0,
  object_count: 0,
  storage_bytes: 0,
};

const PROPS = { owner: "ada", name: "hello", can_admin: true, archived: false };

async function waitForCheckbox(ms = 3_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector('[data-slot="checkbox"]');
    if (el) return el;
    await new Promise((r) => setTimeout(r, 25));
  }
  throw new Error(`vcs checkbox never mounted. ${debugBody()}`);
}

async function waitForSaveButton(ms = 3_000): Promise<HTMLButtonElement> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const buttons = Array.from(document.querySelectorAll("button"));
    const el = buttons.find((b) => /Save protocol setting|Saving…/.test(b.textContent ?? ""));
    if (el) return el as HTMLButtonElement;
    await new Promise((r) => setTimeout(r, 25));
  }
  throw new Error(`save button never mounted. ${debugBody()}`);
}

async function waitUntil(pred: () => boolean, ms = 3_000): Promise<void> {
  const deadline = Date.now() + ms;
  while (!pred() && Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 25));
  }
}

beforeEach(() => {
  statusMock.mockReset();
  enableMock.mockReset();
  statusMock.mockResolvedValue({ ok: true, data: STATUS_OFF });
  enableMock.mockResolvedValue({ ok: true, data: { enabled: true } });
});

afterEach(cleanupBrowserMount);

describe("vcs-settings-panel", () => {
  it("checkbox toggle + Save calls repoEnable without DOM races", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(VcsSettingsPanel, PROPS);
      await waitForCheckbox();
      await clickAriaLabel("Enable the VCS protocol for this repository");
      const save = await waitForSaveButton();
      await act(async () => {
        save.click();
      });
      await waitUntil(() => enableMock.mock.calls.length > 0);
      expect(enableMock).toHaveBeenCalledWith({ owner: "ada", name: "hello", enabled: true });
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });

  it("save failure renders the inline alert without an overlay", async () => {
    enableMock.mockResolvedValue({
      ok: false,
      error: { code: "vcs.forbidden", message: "Not allowed." },
    });
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(VcsSettingsPanel, PROPS);
      await waitForCheckbox();
      await clickAriaLabel("Enable the VCS protocol for this repository");
      const save = await waitForSaveButton();
      await act(async () => {
        save.click();
      });
      await waitUntil(() => document.querySelector('[role="alert"]') !== null);
      expect(document.querySelector('[role="alert"]')?.textContent).toContain("Not allowed.");
      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
