/**
 * Chromium gate for /admin/oxpilot (BR2.6): mount + Switch toggle +
 * provider Select + save must not throw Octane DOM reconciliation
 * errors.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
  pickSelectOptionByTestId,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const meMock = vi.fn();
const settingsGetMock = vi.fn();
const settingsSetMock = vi.fn();
const providersListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      oxpilot: {
        settingsGet: (...args: unknown[]) => settingsGetMock(...args),
        settingsSet: (...args: unknown[]) => settingsSetMock(...args),
        providersList: (...args: unknown[]) => providersListMock(...args),
      },
    },
  },
}));

vi.mock("@/lib/toast", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/toast")>();
  return {
    ...actual,
    toastSuccess: vi.fn(),
    toastError: vi.fn(),
    toastWarning: vi.fn(),
  };
});

// Route module imports ssr-auth → tanstack-start; stub so Chromium Vite never loads Start.
const fetchSessionMeMock = vi.hoisted(() => vi.fn());
const fetchAdminOxpilotSettingsMock = vi.hoisted(() => vi.fn());
const fetchAdminOxpilotProvidersMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/ssr-auth", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/ssr-auth")>();
  return {
    ...actual,
    fetchSessionMe: fetchSessionMeMock,
    fetchAdminOxpilotSettings: fetchAdminOxpilotSettingsMock,
    fetchAdminOxpilotProviders: fetchAdminOxpilotProvidersMock,
  };
});

// Avoid importing Start/router entry points in the Chromium iframe.

import { AdminOxpilotPage } from "./oxpilot";

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

const stubSettings = {
  enabled: true,
  enabled_overridden: false,
  provider_id: "stub",
  provider_id_overridden: false,
  model: "stub-1",
  model_overridden: false,
  base_url: null,
  base_url_overridden: false,
  user_budget: null,
  user_budget_overridden: false,
  instance_budget: null,
  instance_budget_overridden: false,
  key_set: false,
  key_last4: null,
  key_source: "none" as const,
  disabled_reason: null,
  updated_at: null,
};

const providers = [
  { id: "openai", models: ["gpt-4.1"], requires_key: true, configured: false },
  { id: "anthropic", models: ["claude-sonnet-4-5"], requires_key: true, configured: false },
  { id: "ollama", models: [], requires_key: false, configured: false },
  { id: "openai-compatible", models: [], requires_key: false, configured: false },
];

beforeEach(() => {
  meMock.mockReset();
  settingsGetMock.mockReset();
  settingsSetMock.mockReset();
  providersListMock.mockReset();
  fetchSessionMeMock.mockReset();
  fetchAdminOxpilotSettingsMock.mockReset();
  fetchAdminOxpilotProvidersMock.mockReset();

  meMock.mockResolvedValue({ ok: true, data: sysAdmin });
  settingsGetMock.mockResolvedValue({ ok: true, data: stubSettings });
  providersListMock.mockResolvedValue({ ok: true, data: { providers } });
  settingsSetMock.mockResolvedValue({ ok: true, data: stubSettings });
  fetchSessionMeMock.mockImplementation(() => meMock());
  fetchAdminOxpilotSettingsMock.mockImplementation(() => settingsGetMock());
  fetchAdminOxpilotProvidersMock.mockImplementation(() => providersListMock());
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

describe("AdminOxpilotPage browser DOM races", () => {
  it("Switch toggle, provider Select, and save do not throw insertBefore", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(AdminOxpilotPage, {});

      await waitForTestId("admin-oxpilot-page");
      expect(document.body.textContent).toMatch(/OxPilot/);

      // Switch is a form field here — flip it off, then save.
      const toggle = await waitForTestId("admin-oxpilot-enabled");
      await act(async () => {
        (toggle as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 100));

      // Base UI Select — the portal open/close is the race-prone path.
      await pickSelectOptionByTestId(
        "admin-oxpilot-provider",
        "admin-oxpilot-provider-option-openai",
      );
      await new Promise((r) => setTimeout(r, 100));

      const form = document.querySelector("form");
      expect(form).toBeTruthy();
      await act(async () => {
        form!.requestSubmit();
      });
      await new Promise((r) => setTimeout(r, 300));
      expect(settingsSetMock).toHaveBeenCalledWith(
        expect.objectContaining({ enabled: false, provider_id: "openai" }),
      );

      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
