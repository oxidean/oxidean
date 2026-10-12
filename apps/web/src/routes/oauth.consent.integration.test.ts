import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * API-03 /oauth/consent — approve/deny drives oauthApp.authorize then
 * navigates to the client's registered redirect URI.
 */

const authorizeMock = vi.fn();
const authorizeInfoMock = vi.fn();
const authMeMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => authMeMock(...args),
    },
    oauthApp: {
      authorize: (...args: unknown[]) => authorizeMock(...args),
      authorizeInfo: (...args: unknown[]) => authorizeInfoMock(...args),
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
  app_name: "test-cli",
  client_id: "oxidean_oc_0123456789abcdef0123456789abcdef",
  redirect_uri: "https://app.example/callback",
  scopes: ["repo", "read:user"],
  owner_username: "devuser",
};

let assign: ReturnType<typeof vi.spyOn>;

function setLocation(path: string) {
  window.history.pushState({}, "", path);
}

const consentUrl =
  `/oauth/consent?client_id=${encodeURIComponent("oxidean_oc_0123456789abcdef0123456789abcdef")}` +
  `&redirect_uri=${encodeURIComponent("https://app.example/callback")}` +
  `&scope=${encodeURIComponent("repo read:user")}&state=xyz&response_type=code`;

beforeEach(() => {
  authorizeMock.mockReset();
  authorizeInfoMock.mockReset();
  authMeMock.mockReset();
  assign = vi.spyOn(window.location, "assign").mockImplementation(() => {});
  setLocation(consentUrl);
  authMeMock.mockResolvedValue({ ok: true, data: verifiedUser });
  authorizeInfoMock.mockResolvedValue({ ok: true, data: info });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function loadModule(): Promise<Record<string, unknown>> {
  return (await import("./oauth.consent")) as Record<string, unknown>;
}

describe("/oauth/consent (API-03)", () => {
  it("renders app, scopes, and authorize/deny controls", async () => {
    const mod = await loadModule();
    const Page = (mod.OAuthConsentPage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(document.body.textContent).toContain("Authorize test-cli");
    });
    expect(document.body.textContent).toContain("devuser");
    expect(document.body.textContent).toContain(info.redirect_uri);
    expect(document.body.textContent).toContain("repo");
    expect(document.body.textContent).toContain("read:user");
    expect(screen.getByRole("button", { name: "Authorize" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Deny" })).toBeInTheDocument();
  }, 30_000);

  it("approve calls oauthApp.authorize and navigates to redirect_to", async () => {
    authorizeMock.mockResolvedValue({
      ok: true,
      data: { redirect_to: "https://app.example/callback?code=oxidean_oac_x&state=xyz" },
    });
    const mod = await loadModule();
    const Page = (mod.OAuthConsentPage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Authorize" })).toBeInTheDocument();
    });
    screen.getByRole("button", { name: "Authorize" }).click();

    await waitFor(() => {
      expect(authorizeMock).toHaveBeenCalledOnce();
    });
    const input = authorizeMock.mock.calls[0]![0] as Record<string, unknown>;
    expect(input.client_id).toBe(info.client_id);
    expect(input.redirect_uri).toBe(info.redirect_uri);
    expect(input.approve).toBe(true);
    expect(input.state).toBe("xyz");
    await waitFor(() => {
      expect(assign).toHaveBeenCalledWith(
        "https://app.example/callback?code=oxidean_oac_x&state=xyz",
      );
    });
  }, 30_000);

  it("deny submits approve:false and redirects with access_denied", async () => {
    authorizeMock.mockResolvedValue({
      ok: true,
      data: { redirect_to: "https://app.example/callback?error=access_denied&state=xyz" },
    });
    const mod = await loadModule();
    const Page = (mod.OAuthConsentPage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Deny" })).toBeInTheDocument();
    });
    screen.getByRole("button", { name: "Deny" }).click();

    await waitFor(() => {
      expect(authorizeMock).toHaveBeenCalledOnce();
    });
    expect((authorizeMock.mock.calls[0]![0] as Record<string, unknown>).approve).toBe(false);
    await waitFor(() => {
      expect(assign).toHaveBeenCalledWith(
        "https://app.example/callback?error=access_denied&state=xyz",
      );
    });
  }, 30_000);

  it("error kind renders the message", async () => {
    authorizeInfoMock.mockResolvedValue({
      ok: false,
      error: { code: "oauth.invalid_request", message: "Invalid authorization request." },
    });
    const mod = await loadModule();
    const Page = (mod.OAuthConsentPage ?? mod.default) as unknown;
    renderWithQueryClient(Page);
    await waitFor(() => {
      expect(document.body.textContent).toContain("Invalid authorization request.");
    });
  }, 30_000);
});
