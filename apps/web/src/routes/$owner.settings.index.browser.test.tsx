/**
 * Chromium gate for /$owner/settings/ — member-base Select inside
 * form.Subscribe must open/pick without insertBefore DOM races.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  clickAriaLabel,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const updateSettingsMock = vi.fn();

const meMock = vi.fn();
const orgGetMock = vi.fn();
const listMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: { me: (...args: unknown[]) => meMock(...args) },
    org: {
      get: (...args: unknown[]) => orgGetMock(...args),
      listMine: (...args: unknown[]) => listMineMock(...args),
      updateSettings: (...args: unknown[]) => updateSettingsMock(...args),
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  // Toaster mounts appToastManager — a bare-object stub keeps it inert.
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

// Avoid importing Start/router entry points in the Chromium iframe.

import { OrgSettingsPage } from "./$owner.settings.index";

const org = {
  id: "o1",
  slug: "acme",
  display_name: "Acme",
  member_base_permission: "none" as const,
};

beforeEach(() => {
  window.history.pushState({}, "", "/acme/settings");
  meMock.mockReset();
  orgGetMock.mockReset();
  listMineMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: { id: "u1", username: "ada" } });
  orgGetMock.mockResolvedValue({ ok: true, data: org });
  listMineMock.mockResolvedValue({
    ok: true,
    data: { orgs: [{ slug: "acme", role: "admin" }] },
  });
  updateSettingsMock.mockReset();
  updateSettingsMock.mockResolvedValue({
    ok: true,
    data: { ...org, member_base_permission: "write" },
  });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForSelector(selector: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

/** Base UI Select.Item needs pointerdown before click commits under Chromium. */
async function pickSelectOptionByText(label: string): Promise<void> {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    const option = Array.from(document.querySelectorAll('[data-slot="select-item"]')).find(
      (el) => (el.textContent ?? "").trim() === label,
    );
    if (option) {
      await act(async () => {
        option.dispatchEvent(
          new PointerEvent("pointerdown", {
            bubbles: true,
            cancelable: true,
            pointerType: "mouse",
          }),
        );
        (option as HTMLElement).click();
      });
      return;
    }
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`select option "${label}" never appeared. ${debugBody()}`);
}

describe("OrgSettingsPage browser DOM races", () => {
  it("member-base Select inside form.Subscribe picks options without insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(OrgSettingsPage, {});

      await waitForSelector('[data-testid="org-settings-general"]');
      await waitForSelector("#org-member-base");

      // Open the member-base Select and commit each option in turn; the
      // Subscribe-wrapped Select re-renders on every value change.
      await clickAriaLabel("Base permission for Members");
      await pickSelectOptionByText("Write");

      await clickAriaLabel("Base permission for Members");
      await pickSelectOptionByText("Read");

      // Submit through the Subscribe'd form to the mocked RPC.
      const submit = Array.from(document.querySelectorAll("button")).find((b) =>
        /^Save settings$/i.test((b.textContent ?? "").trim()),
      );
      expect(submit).toBeTruthy();
      await act(async () => {
        (submit as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 150));
      expect(updateSettingsMock).toHaveBeenCalledWith(
        expect.objectContaining({ slug: "acme", member_base_permission: "read" }),
      );

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 30_000);
});
