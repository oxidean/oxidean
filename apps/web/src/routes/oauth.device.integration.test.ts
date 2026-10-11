import { createElement } from "octane";
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

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    oauth: {
      deviceInfo: (...args: unknown[]) => deviceInfoMock(...args),
      deviceAuthorize: (...args: unknown[]) => deviceAuthorizeMock(...args),
    },
  },
}));

type LoaderShape = { kind: "unauthenticated"; returnTo: string } | { kind: "ready" };

let loaderData: LoaderShape;
let searchParams: Record<string, string | undefined> = {};

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useLoaderData: () => loaderData,
    useSearch: () => searchParams,
    Link: (props: {
      to?: string;
      children?: unknown;
      className?: string;
      "aria-current"?: string;
    }) =>
      createElement(
        "a",
        {
          href: props.to ?? "#",
          className: props.className,
          "aria-current": props["aria-current"],
        },
        props.children as never,
      ),
  };
});

const info = {
  user_code: "WDJB-MJHT",
  scopes: ["repo", "package:read"],
  expires_at: "2030-01-01T00:00:00Z",
};

const assign = vi.fn();

beforeEach(() => {
  deviceInfoMock.mockReset();
  deviceAuthorizeMock.mockReset();
  assign.mockReset();
  vi.stubGlobal("location", { assign });
  loaderData = { kind: "ready" };
  searchParams = {};
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function loadModule(): Promise<Record<string, unknown>> {
  return (await import("./oauth.device")) as Record<string, unknown>;
}

describe("/oauth/device", () => {
  it("auto-looks up ?user_code= and renders scopes with approve/deny", async () => {
    deviceInfoMock.mockResolvedValue({ ok: true, data: info });
    searchParams = { user_code: "wdjb-mjht" };
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
    searchParams = { user_code: info.user_code };
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
    searchParams = { user_code: info.user_code };
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
    searchParams = { user_code: "ZZZZ-ZZZZ" };
    const mod = await loadModule();
    const Page = (mod.OAuthDevicePage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(document.body.textContent).toContain("no pending device code");
    });
    expect(screen.getByTestId("oauth-device-code")).toBeInTheDocument();
  }, 30_000);
});
