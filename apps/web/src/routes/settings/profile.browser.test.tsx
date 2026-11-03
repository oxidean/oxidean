/**
 * Chromium gate: /settings/profile "Include private contributions" Checkbox
 * (#COL-12) must toggle/commit via user.updateProfile without insertBefore /
 * Octane overlay races.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanupBrowserMount,
  debugBody,
  expectNoOctaneOverlayInDocument,
  mountWithQueryClient,
} from "@/test/browser-mount";
import { trackDomErrors } from "@/test/dom-errors";
import { act } from "octane";

const meMock = vi.fn();
const getProfileMock = vi.fn();
const updateProfileMock = vi.fn();
const emailListMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      providerConfig: vi.fn(async () => ({
        ok: true,
        data: { mode: "local", allow_signup: true },
      })),
      bootstrapStatus: vi.fn(async () => ({ ok: true, data: { needs_setup: false } })),
    },
    user: {
      getProfile: (...args: unknown[]) => getProfileMock(...args),
      updateProfile: (...args: unknown[]) => updateProfileMock(...args),
    },
    email: {
      list: (...args: unknown[]) => emailListMock(...args),
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
  toastWarning: vi.fn(),
}));

// ProfileAvatarField pulls @octanejs/dropzone → attr-accept, whose ESM build
// is broken under vitest browser deps — the avatar path is not the gated
// surface here (stack-browser settings-profile-avatar covers it).
vi.mock("@/components/settings/profile-avatar-field", () => ({
  ProfileAvatarField: () => null,
}));

import { ProfilePage } from "./profile";

const user = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  default_branch: "main",
  two_factor_enabled: false,
  two_factor_enroll_required: false,
  show_private_contributions: true,
};

beforeEach(() => {
  window.history.pushState({}, "", "/settings/profile");
  meMock.mockReset();
  getProfileMock.mockReset();
  updateProfileMock.mockReset();
  emailListMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: user });
  getProfileMock.mockResolvedValue({ ok: true, data: user });
  emailListMock.mockResolvedValue({ ok: true, data: [] });
  updateProfileMock.mockImplementation(async (input: Record<string, unknown>) => ({
    ok: true,
    data: { ...user, ...input },
  }));
});

afterEach(async () => {
  await cleanupBrowserMount();
});

async function waitForSelector(selector: string, ms = 15_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`${selector} never appeared. ${debugBody()}`);
}

describe("/settings/profile browser DOM races", () => {
  it("show-private-contributions Checkbox toggles + commits cleanly", async () => {
    const tracker = trackDomErrors();
    try {
      await mountWithQueryClient(ProfilePage);
      await waitForSelector('[data-testid="settings-profile-page"]');
      const checkbox = await waitForSelector("#show-private-contributions");

      // Toggle off → commit via updateProfile(show_private_contributions:false).
      await act(async () => {
        (checkbox as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 150));
      expect(updateProfileMock).toHaveBeenCalledWith(
        expect.objectContaining({ show_private_contributions: false }),
      );

      // Toggle back on through the re-rendered control.
      const again = await waitForSelector("#show-private-contributions");
      await act(async () => {
        (again as HTMLElement).click();
      });
      await new Promise((r) => setTimeout(r, 150));
      expect(updateProfileMock).toHaveBeenCalledWith(
        expect.objectContaining({ show_private_contributions: true }),
      );

      tracker.expectNoDomRaces();
      expectNoOctaneOverlayInDocument();
    } finally {
      tracker.dispose();
    }
  }, 40_000);
});
