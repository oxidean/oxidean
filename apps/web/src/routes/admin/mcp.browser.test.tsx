/**
 * Chromium gate for /admin/mcp Switch (AGT-03): mount + toggle + env-default
 * reset must not throw Octane DOM reconciliation errors.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  clickTestId,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const meMock = vi.fn();
const getSettingsMock = vi.fn();
const updateSettingsMock = vi.fn();

const loaderState = vi.hoisted(() => {
  let data: unknown;
  return {
    get: () => data,
    set: (next: unknown) => {
      data = next;
    },
  };
});

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      mcp: {
        getSettings: (...args: unknown[]) => getSettingsMock(...args),
        updateSettings: (...args: unknown[]) => updateSettingsMock(...args),
      },
    },
  },
}));

vi.mock("@/lib/toast", () => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
  toastWarning: vi.fn(),
}));

// Route module imports ssr-auth → tanstack-start; stub so Chromium Vite never loads Start.
vi.mock("@/lib/ssr-auth", () => ({
  fetchSessionMe: vi.fn(),
  fetchAdminMcpSettings: vi.fn(),
}));

// Avoid importing Start/router entry points in the Chromium iframe.
vi.mock("@octanejs/tanstack-router", () => ({
  createFileRoute: () => (opts: unknown) => opts,
  useLoaderData: () => loaderState.get(),
}));

import { AdminMcpPage } from "./mcp";

const sysAdmin = {
  id: "u1",
  email: "admin@example.com",
  username: "admin",
  display_name: "Admin",
  bio: "",
  avatar_url: null as null,
  role: "sys-admin" as const,
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  default_branch: "main",
};

const enabledSettings = { enabled: true, enabled_overridden: true };
const disabledSettings = { enabled: false, enabled_overridden: true };

beforeEach(() => {
  meMock.mockReset();
  getSettingsMock.mockReset();
  updateSettingsMock.mockReset();

  meMock.mockResolvedValue({ ok: true, data: sysAdmin });
  getSettingsMock.mockResolvedValue({ ok: true, data: enabledSettings });
  updateSettingsMock.mockResolvedValue({ ok: true, data: disabledSettings });

  loaderState.set({
    kind: "ready",
    me: sysAdmin,
    settings: enabledSettings,
  });
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

describe("AdminMcpPage browser DOM races", () => {
  it("Switch toggle and env-default reset do not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(AdminMcpPage, {});

      await waitForTestId("admin-mcp-page");
      const toggle = await waitForTestId("admin-mcp-enabled");
      expect(document.body.textContent).toMatch(/MCP endpoint/);

      // Switch is a Base UI button — click toggles and calls updateSettings.
      await act(async () => {
        (toggle as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 100));
      expect(updateSettingsMock).toHaveBeenCalledWith({ enabled: false });

      // The override-active state renders the env-default reset button.
      await waitForTestId("admin-mcp-clear-override");
      await clickTestId("admin-mcp-clear-override");
      await new Promise((r) => setTimeout(r, 100));
      expect(updateSettingsMock).toHaveBeenCalledWith({ clear_overrides: true });

      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
