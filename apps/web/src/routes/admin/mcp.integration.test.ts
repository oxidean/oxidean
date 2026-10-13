import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const getSettingsMock = vi.fn();
const updateSettingsMock = vi.fn();

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

const envDefaultSettings = {
  enabled: true,
  enabled_overridden: false,
};

const overriddenSettings = {
  enabled: false,
  enabled_overridden: true,
};

const assignMock = vi.fn();

import { AdminMcpPage } from "./mcp";

/**
 * AGT-03: /admin/mcp instance endpoint toggle must render and drive
 * admin.mcp.updateSettings (Switch is high-risk — paired with the browser test).
 */
describe("/admin/mcp", () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    meMock.mockReset();
    getSettingsMock.mockReset();
    updateSettingsMock.mockReset();
    assignMock.mockReset();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        assign: assignMock,
        href: "http://localhost/admin/mcp",
        origin: "http://localhost",
        pathname: "/admin/mcp",
        search: "",
        replace: vi.fn(),
        reload: vi.fn(),
      },
    });

    meMock.mockResolvedValue({ ok: true, data: sysAdmin });
    getSettingsMock.mockResolvedValue({ ok: true, data: envDefaultSettings });
    updateSettingsMock.mockResolvedValue({ ok: true, data: overriddenSettings });
  });

  it("exports AdminMcpPage without @else if and wires getSettings/updateSettings", async () => {
    const mod = await import("./mcp");
    expect(mod.AdminMcpPage ?? mod.default).toBeTruthy();
    const src = await import("./mcp.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/getSettings/);
    expect(src).toMatch(/updateSettings/);
    expect(src).toMatch(/fetchAdminMcpSettings|loader:/);
    expect(src).toMatch(/initialData/);
    expect(src).toMatch(/OXIDEAN_MCP_ENABLED/);
    expect(src).toMatch(/clear_overrides/);
    expect(src).toMatch(/toastError|toastSuccess/);
    expect(src).toMatch(/kind: "error"|setPending|\[pending,/);
    expect(src).not.toMatch(/@else if/);
    expect(src).not.toMatch(/Loading…/);
  });

  it("renders the toggle, status, and admin nav for sys-admin", async () => {
    renderWithQueryClient(AdminMcpPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-mcp-page")).toBeTruthy();
        expect(screen.getByRole("heading", { name: "MCP endpoint" })).toBeTruthy();
        expect(screen.getByTestId("admin-mcp-status")).toBeTruthy();
        expect(screen.getByTestId("admin-mcp-status").textContent).toMatch(/Enabled/);
        expect(screen.getByTestId("admin-mcp-status").textContent).toMatch(/environment default/);
        expect(screen.getByTestId("admin-mcp-enabled")).toBeTruthy();
      },
      { timeout: 10_000 },
    );

    const nav = screen.getByRole("navigation", { name: "Admin settings" });
    expect(nav.querySelector('a[href="/admin/mcp"]')).toBeTruthy();
    expect(screen.queryByTestId("admin-mcp-clear-override")).toBeNull();
    expect(screen.queryByText("Loading…")).toBeNull();

    // Endpoint URL row + pointer to the user-facing MCP setup page.
    const endpoint = screen.getByRole("textbox", { name: "Copy MCP endpoint URL" });
    expect((endpoint as HTMLInputElement).value).toMatch(/\/api\/mcp$/);
    expect(screen.getAllByRole("link").some((l) => l.getAttribute("href") === "/settings/ai")).toBe(
      true,
    );
  });

  it("oxidean:public-origin meta overrides the location-derived endpoint URL", async () => {
    const meta = document.createElement("meta");
    meta.setAttribute("name", "oxidean:public-origin");
    meta.setAttribute("content", "https://forge.example");
    document.head.appendChild(meta);

    try {
      renderWithQueryClient(AdminMcpPage);
      await waitFor(() => {
        const endpoint = screen.getByRole("textbox", { name: "Copy MCP endpoint URL" });
        expect((endpoint as HTMLInputElement).value).toBe("https://forge.example/api/mcp");
      });
    } finally {
      meta.remove();
    }
  });

  it("toggling the Switch calls updateSettings with the new enabled value", async () => {
    renderWithQueryClient(AdminMcpPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-mcp-enabled")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("admin-mcp-enabled"));

    await waitFor(() => {
      expect(updateSettingsMock).toHaveBeenCalledWith({ enabled: false });
    });
  });

  it("probe POSTs initialize to /api/mcp and reports the handshake", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        jsonrpc: "2.0",
        id: "admin-mcp-probe",
        result: {
          protocolVersion: "2025-06-18",
          capabilities: {},
          serverInfo: { name: "oxidean", version: "1.2.3" },
        },
      }),
    });
    vi.stubGlobal("fetch", fetchMock);

    renderWithQueryClient(AdminMcpPage);
    await waitFor(() => {
      expect(screen.getByTestId("admin-mcp-probe")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("admin-mcp-probe"));

    await waitFor(() => {
      const out = screen.getByTestId("admin-mcp-probe-result");
      expect(out.getAttribute("data-probe-kind")).toBe("up");
      expect(out.textContent).toMatch(/Up — oxidean 1\.2\.3/);
      expect(out.textContent).toMatch(/protocol 2025-06-18/);
    });
    expect(fetchMock).toHaveBeenCalledWith("/api/mcp", expect.objectContaining({ method: "POST" }));
  });

  it("probe reports mcp.disabled when the endpoint is off", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        json: async () => ({
          jsonrpc: "2.0",
          id: null,
          error: {
            code: -32000,
            message: "mcp.disabled: the MCP endpoint is disabled on this instance",
          },
        }),
      }),
    );

    renderWithQueryClient(AdminMcpPage);
    await waitFor(() => {
      expect(screen.getByTestId("admin-mcp-probe")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("admin-mcp-probe"));

    await waitFor(() => {
      const out = screen.getByTestId("admin-mcp-probe-result");
      expect(out.getAttribute("data-probe-kind")).toBe("down");
      expect(out.textContent).toMatch(/mcp\.disabled/);
    });
  });

  it("shows the env-default reset when the override is active", async () => {
    getSettingsMock.mockResolvedValue({ ok: true, data: overriddenSettings });
    updateSettingsMock.mockResolvedValue({ ok: true, data: envDefaultSettings });

    renderWithQueryClient(AdminMcpPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-mcp-clear-override")).toBeTruthy();
        expect(screen.getByTestId("admin-mcp-status").textContent).toMatch(/admin override/);
        expect(screen.getByTestId("admin-mcp-status").textContent).toMatch(/Disabled/);
      },
      { timeout: 10_000 },
    );

    fireEvent.click(screen.getByTestId("admin-mcp-clear-override"));

    await waitFor(() => {
      expect(updateSettingsMock).toHaveBeenCalledWith({ clear_overrides: true });
    });
  });

  it("redirects signed-out sessions toward login", async () => {
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });

    renderWithQueryClient(AdminMcpPage);

    await waitFor(() => {
      expect(assignMock).toHaveBeenCalledWith("/login?returnTo=/admin/mcp");
    });
    expect(screen.getByText(/Sign in as a system administrator/i)).toBeTruthy();
    expect(screen.getByRole("link", { name: /Sign in/i })).toHaveAttribute(
      "href",
      "/login?returnTo=/admin/mcp",
    );
  });

  it("shows forbidden for non sys-admin from loader", async () => {
    meMock.mockResolvedValue({
      ok: true,
      data: { ...sysAdmin, role: "user" },
    });

    renderWithQueryClient(AdminMcpPage);

    await waitFor(() => {
      expect(screen.getByText(/You need admin access to manage MCP settings/i)).toBeTruthy();
    });
    expect(getSettingsMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("admin-mcp-page")).toBeNull();
  });
});
