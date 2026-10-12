import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * /oauth/device — RFC 8628 approval page for `ox auth login --web`.
 * Code lookup drives oauth.deviceInfo; approve/deny drives
 * oauth.deviceAuthorize.
 */

const deviceInfoMock = vi.fn();
const deviceAuthorizeMock = vi.fn();
const authMeMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => authMeMock(...args),
    },
    oauth: {
      deviceInfo: (...args: unknown[]) => deviceInfoMock(...args),
      deviceAuthorize: (...args: unknown[]) => deviceAuthorizeMock(...args),
    },
  },
}));

const verifiedUser = {
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
};

const info = {
  user_code: "WDJB-MJHT",
  scopes: ["repo", "package:read"],
  expires_at: "2030-01-01T00:00:00Z",
};

function setLocation(path: string) {
  window.history.pushState({}, "", path);
}

beforeEach(() => {
  deviceInfoMock.mockReset();
  deviceAuthorizeMock.mockReset();
  authMeMock.mockReset();
  setLocation("/oauth/device");
  authMeMock.mockResolvedValue({ ok: true, data: verifiedUser });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function loadModule(): Promise<Record<string, unknown>> {
  return (await import("./oauth.device")) as Record<string, unknown>;
}

describe("/oauth/device", () => {
  it("auto-looks up ?user_code= and renders scopes with approve/deny", async () => {
    deviceInfoMock.mockResolvedValue({ ok: true, data: info });
    setLocation("/oauth/device?user_code=wdjb-mjht");
    const mod = await loadModule();
    const Page = (mod.OAuthDevicePage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(document.body.textContent).toContain("WDJB-MJHT");
    });
    await waitFor(() => {
      expect(deviceInfoMock).toHaveBeenCalledWith({ user_code: "wdjb-mjht" });
    });
    expect(document.body.textContent).toContain("repo");
    expect(screen.getByRole("button", { name: "Authorize ox" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Deny" })).toBeInTheDocument();
  }, 30_000);

  it("approve calls deviceAuthorize with approve:true", async () => {
    deviceInfoMock.mockResolvedValue({ ok: true, data: info });
    deviceAuthorizeMock.mockResolvedValue({ ok: true, data: { ok: true } });
    setLocation(`/oauth/device?user_code=${info.user_code}`);
    const mod = await loadModule();
    const Page = (mod.OAuthDevicePage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Authorize ox" })).toBeInTheDocument();
    });
    screen.getByRole("button", { name: "Authorize ox" }).click();

    await waitFor(() => {
      expect(deviceAuthorizeMock).toHaveBeenCalledWith({
        user_code: "WDJB-MJHT",
        approve: true,
      });
    });
    await waitFor(() => {
      expect(document.body.textContent).toContain("CLI sign-in authorized");
    });
  }, 30_000);

  it("deny calls deviceAuthorize with approve:false", async () => {
    deviceInfoMock.mockResolvedValue({ ok: true, data: info });
    deviceAuthorizeMock.mockResolvedValue({ ok: true, data: { ok: true } });
    setLocation(`/oauth/device?user_code=${info.user_code}`);
    const mod = await loadModule();
    const Page = (mod.OAuthDevicePage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Deny" })).toBeInTheDocument();
    });
    screen.getByRole("button", { name: "Deny" }).click();

    await waitFor(() => {
      expect(deviceAuthorizeMock).toHaveBeenCalledWith({
        user_code: "WDJB-MJHT",
        approve: false,
      });
    });
    await waitFor(() => {
      expect(document.body.textContent).toContain("Sign-in denied");
    });
  }, 30_000);

  it("unknown code renders the error and the manual entry form stays up", async () => {
    deviceInfoMock.mockResolvedValue({
      ok: false,
      error: { code: "oauth.device_code_not_found", message: "no pending device code" },
    });
    setLocation("/oauth/device?user_code=ZZZZ-ZZZZ");
    const mod = await loadModule();
    const Page = (mod.OAuthDevicePage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(document.body.textContent).toContain("no pending device code");
    });
    expect(screen.getByTestId("oauth-device-code")).toBeInTheDocument();
  }, 30_000);
});
