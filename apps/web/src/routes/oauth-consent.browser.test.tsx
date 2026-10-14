/**
 * Chromium gate for the OAuth consent + device-approval scope checkboxes:
 * mount, uncheck/re-check, approve-disable-when-empty, and overlay/DOM-race
 * asserts. jsdom can't drive Base UI's click chain (root onClick forwards to
 * a hidden input), so narrowing behavior is proven here; submission payloads
 * stay covered by the *.integration.test.ts suites.
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

const authorizeMock = vi.fn();
const authorizeInfoMock = vi.fn();
const deviceInfoMock = vi.fn();
const deviceAuthorizeMock = vi.fn();
const meMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    oauthApp: {
      authorize: (...args: unknown[]) => authorizeMock(...args),
      authorizeInfo: (...args: unknown[]) => authorizeInfoMock(...args),
    },
    oauth: {
      deviceInfo: (...args: unknown[]) => deviceInfoMock(...args),
      deviceAuthorize: (...args: unknown[]) => deviceAuthorizeMock(...args),
    },
  },
}));

// Route modules import ssr-auth → tanstack-start; stub so the Chromium iframe
// never loads Start.
const fetchSessionMeMock = vi.hoisted(() => vi.fn());
const fetchOAuthAuthorizeInfoMock = vi.hoisted(() => vi.fn());
vi.mock("@/lib/ssr-auth", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/ssr-auth")>();
  return {
    ...actual,
    fetchSessionMe: fetchSessionMeMock,
    fetchOAuthAuthorizeInfo: fetchOAuthAuthorizeInfoMock,
  };
});

import { OAuthConsentPage } from "./oauth.consent";
import { OAuthDevicePage } from "./oauth.device";

const verifiedUser = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null as null,
  role: "user" as const,
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

const consentInfo = {
  app_name: "test-cli",
  client_id: "oxidean_oc_0123456789abcdef0123456789abcdef",
  redirect_uri: "https://app.example/callback",
  scopes: ["repo", "read:user"],
  owner_username: "devuser",
};

const deviceInfo = {
  user_code: "WDJB-MJHT",
  scopes: ["repo", "package:read"],
  expires_at: "2030-01-01T00:00:00Z",
};

async function waitForTestId(testId: string, ms = 10_000): Promise<Element> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    const el = document.querySelector(`[data-testid="${testId}"]`);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  throw new Error(`[data-testid="${testId}"] never appeared. ${debugBody()}`);
}

function checkboxState(testId: string): string | null {
  return document.querySelector(`[data-testid="${testId}"]`)?.getAttribute("aria-checked");
}

function approveDisabled(testId: string): boolean {
  return (
    (document.querySelector(`[data-testid="${testId}"]`) as HTMLButtonElement | null)?.disabled ??
    false
  );
}

beforeEach(() => {
  authorizeMock.mockReset();
  authorizeInfoMock.mockReset();
  deviceInfoMock.mockReset();
  deviceAuthorizeMock.mockReset();
  meMock.mockReset();
  fetchSessionMeMock.mockReset();
  fetchOAuthAuthorizeInfoMock.mockReset();

  meMock.mockResolvedValue({ ok: true, data: verifiedUser });
  fetchSessionMeMock.mockImplementation(() => meMock());
  authorizeInfoMock.mockResolvedValue({ ok: true, data: consentInfo });
  fetchOAuthAuthorizeInfoMock.mockImplementation(() => authorizeInfoMock());
  deviceInfoMock.mockResolvedValue({ ok: true, data: deviceInfo });
  authorizeMock.mockResolvedValue({
    ok: true,
    data: { redirect_to: "https://app.example/callback?code=oxidean_oac_x&state=xyz" },
  });
  deviceAuthorizeMock.mockResolvedValue({ ok: true, data: { ok: true } });
});

afterEach(async () => {
  await cleanupBrowserMount();
});

describe("OAuth consent scope checkboxes (oauth.consent)", () => {
  it("uncheck narrows the grant; all-unchecked disables Authorize", async () => {
    const tracker = trackDomErrors();
    try {
      window.history.pushState(
        {},
        "",
        `/oauth/consent?client_id=${consentInfo.client_id}` +
          `&redirect_uri=${encodeURIComponent(consentInfo.redirect_uri)}` +
          `&scope=${encodeURIComponent("repo read:user")}&state=xyz&response_type=code`,
      );
      await mountWithQueryClient(OAuthConsentPage, {});

      await waitForTestId("oauth-scope-repo");
      expect(checkboxState("oauth-scope-repo")).toBe("true");
      expect(checkboxState("oauth-scope-read:user")).toBe("true");

      // Uncheck one → still grantable; narrowed set submits as granted_scope.
      await clickTestId("oauth-scope-repo");
      expect(checkboxState("oauth-scope-repo")).toBe("false");
      expect(approveDisabled("oauth-consent-approve")).toBe(false);

      // Uncheck the last one → approve blocked, deny still live.
      await clickTestId("oauth-scope-read:user");
      expect(approveDisabled("oauth-consent-approve")).toBe(true);
      expect(
        (document.querySelector('[data-testid="oauth-consent-deny"]') as HTMLButtonElement)
          .disabled,
      ).toBe(false);

      // Re-check restores approval.
      await clickTestId("oauth-scope-repo");
      expect(approveDisabled("oauth-consent-approve")).toBe(false);

      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});

describe("OAuth device approval scope checkboxes (oauth.device)", () => {
  it("uncheck narrows granted_scope; all-unchecked disables approve", async () => {
    const tracker = trackDomErrors();
    try {
      window.history.pushState({}, "", "/oauth/device?user_code=wdjb-mjht");
      await mountWithQueryClient(OAuthDevicePage, {});

      await waitForTestId("oauth-scope-repo");
      expect(checkboxState("oauth-scope-repo")).toBe("true");
      expect(checkboxState("oauth-scope-package:read")).toBe("true");

      await clickTestId("oauth-scope-repo");
      expect(checkboxState("oauth-scope-repo")).toBe("false");
      expect(approveDisabled("oauth-device-approve")).toBe(false);

      await clickTestId("oauth-scope-package:read");
      expect(approveDisabled("oauth-device-approve")).toBe(true);

      expectNoOctaneOverlayInDocument();
      tracker.expectNoDomRaces();
    } finally {
      tracker.dispose();
    }
  });
});
