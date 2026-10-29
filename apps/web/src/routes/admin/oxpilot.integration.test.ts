import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

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

const openaiSettings = {
  ...stubSettings,
  provider_id: "openai",
  provider_id_overridden: true,
  model: "gpt-4.1",
  key_source: "env" as const,
};

// Mirrors the real providers_list surface — REGISTRY omits `stub`
// (sys-admin preview mode, not admin-settable) and lists compat last.
const providers = [
  { id: "openai", models: ["gpt-4.1"], requires_key: true, configured: false },
  { id: "anthropic", models: ["claude-sonnet-4-5"], requires_key: true, configured: false },
  { id: "ollama", models: [], requires_key: false, configured: false },
  { id: "openai-compatible", models: [], requires_key: false, configured: false },
];

const assignMock = vi.fn();

import { AdminOxpilotPage } from "./oxpilot";

/**
 * /admin/oxpilot — the sys-admin provider surface (BR2.6): stub default,
 * provider/model/base-url/key writes, budgets, env-default reset.
 */
describe("/admin/oxpilot", () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    meMock.mockReset();
    settingsGetMock.mockReset();
    settingsSetMock.mockReset();
    providersListMock.mockReset();
    assignMock.mockReset();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        assign: assignMock,
        href: "http://localhost/admin/oxpilot",
        origin: "http://localhost",
        pathname: "/admin/oxpilot",
        search: "",
        replace: vi.fn(),
        reload: vi.fn(),
      },
    });

    meMock.mockResolvedValue({ ok: true, data: sysAdmin });
    settingsGetMock.mockResolvedValue({ ok: true, data: stubSettings });
    providersListMock.mockResolvedValue({ ok: true, data: { providers } });
    settingsSetMock.mockResolvedValue({ ok: true, data: stubSettings });
  });

  it("wires the admin.oxpilot procs and uses no @else if", async () => {
    const mod = await import("./oxpilot");
    expect(mod.AdminOxpilotPage).toBeTruthy();
    const src = await import("./oxpilot.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/admin\.oxpilot\.settingsSet/);
    expect(src).toMatch(/clear_overrides/);
    expect(src).toMatch(/clear_key/);
    expect(src).toMatch(/fetchAdminOxpilotSettings/);
    expect(src).toMatch(/fetchAdminOxpilotProviders/);
    expect(src).not.toMatch(/@else if/);
  });

  it("renders the form, stub notice, and admin nav for sys-admin", async () => {
    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-oxpilot-page")).toBeTruthy();
        expect(screen.getByRole("heading", { name: "OxPilot" })).toBeTruthy();
        expect(screen.getByTestId("admin-oxpilot-status").textContent).toMatch(
          /Enabled — provider: stub/,
        );
        expect(screen.getByTestId("admin-oxpilot-enabled")).toBeTruthy();
        expect(screen.getByTestId("admin-oxpilot-provider")).toBeTruthy();
        expect(screen.getByTestId("admin-oxpilot-model")).toBeTruthy();
        expect(screen.getByTestId("admin-oxpilot-save")).toBeTruthy();
      },
      { timeout: 10_000 },
    );

    // The stub restriction is named so admins know the blast radius.
    expect(screen.getByText(/restricted to system administrators/)).toBeTruthy();
    // Nav carries the new entry.
    const nav = screen.getByRole("navigation", { name: "Admin settings" });
    expect(nav.querySelector('a[href="/admin/oxpilot"]')).toBeTruthy();
    // No overrides active → no reset button.
    expect(screen.queryByTestId("admin-oxpilot-clear-overrides")).toBeNull();
  });

  it("save calls settingsSet with the form values", async () => {
    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-oxpilot-save")).toBeTruthy();
    });

    fireEvent.input(screen.getByTestId("admin-oxpilot-model"), {
      target: { value: "stub-2" },
    });
    fireEvent.input(screen.getByTestId("admin-oxpilot-user-budget"), {
      target: { value: "500" },
    });

    fireEvent.submit(screen.getByTestId("admin-oxpilot-save").closest("form")!);

    await waitFor(() => {
      expect(settingsSetMock).toHaveBeenCalledWith({
        enabled: true,
        // "Environment default" → provider_id: null; the server resolves
        // env ?? stub. `stub` itself is never a concrete write (release
        // refuses it — sys-admin-only preview mode).
        provider_id: null,
        model: "stub-2",
        base_url: null,
        user_budget: 500,
        instance_budget: null,
      });
    });
  });

  it("does not send api_key when the key field is blank", async () => {
    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-oxpilot-save")).toBeTruthy();
    });

    fireEvent.submit(screen.getByTestId("admin-oxpilot-save").closest("form")!);

    await waitFor(() => {
      expect(settingsSetMock).toHaveBeenCalled();
    });
    const sent = settingsSetMock.mock.calls[0]![0] as Record<string, unknown>;
    expect("api_key" in sent).toBe(false);
  });

  it("refuses a non-positive budget without calling settingsSet", async () => {
    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-oxpilot-save")).toBeTruthy();
    });

    fireEvent.input(screen.getByTestId("admin-oxpilot-instance-budget"), {
      target: { value: "0" },
    });
    fireEvent.submit(screen.getByTestId("admin-oxpilot-save").closest("form")!);

    // Give the submit path a beat — the write must never fire.
    await new Promise((r) => setTimeout(r, 300));
    expect(settingsSetMock).not.toHaveBeenCalled();
  });

  it("clear overrides → settingsSet({ clear_overrides: true })", async () => {
    settingsGetMock.mockResolvedValue({
      ok: true,
      data: { ...stubSettings, provider_id_overridden: true },
    });
    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-oxpilot-clear-overrides")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("admin-oxpilot-clear-overrides"));

    await waitFor(() => {
      expect(settingsSetMock).toHaveBeenCalledWith({ clear_overrides: true });
    });
  });

  it("clear key → settingsSet({ clear_key: true })", async () => {
    settingsGetMock.mockResolvedValue({
      ok: true,
      data: { ...stubSettings, key_set: true, key_last4: "wxyz", key_source: "db" as const },
    });
    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-oxpilot-clear-key")).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId("admin-oxpilot-clear-key"));

    await waitFor(() => {
      expect(settingsSetMock).toHaveBeenCalledWith({ clear_key: true });
    });
  });

  it("non-stub provider hides the sys-admin-only notice", async () => {
    settingsGetMock.mockResolvedValue({ ok: true, data: openaiSettings });
    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-oxpilot-status").textContent).toMatch(
        /Enabled — provider: openai/,
      );
    });
    expect(screen.queryByText(/restricted to system administrators/)).toBeNull();
    expect(screen.getByText(/This provider needs an API key/)).toBeTruthy();
  });

  it("env-default selection writes provider_id: null back to the resolved stub", async () => {
    // The resolved provider is stub (env unset) — the select shows
    // "Environment default", and saving never names `stub` in the write.
    renderWithQueryClient(AdminOxpilotPage);
    await waitFor(() => {
      expect(screen.getByTestId("admin-oxpilot-provider")).toBeTruthy();
    });
    expect(screen.getByText(/Environment default \(Stub/)).toBeTruthy();

    fireEvent.submit(screen.getByTestId("admin-oxpilot-save").closest("form")!);
    await waitFor(() => {
      expect(settingsSetMock).toHaveBeenCalled();
    });
    const sent = settingsSetMock.mock.calls[0]![0] as Record<string, unknown>;
    expect(sent.provider_id).toBeNull();
  });

  it("openai-compatible requires a base_url — blank save is refused locally", async () => {
    settingsGetMock.mockResolvedValue({
      ok: true,
      data: {
        ...stubSettings,
        provider_id: "openai-compatible",
        provider_id_overridden: true,
        model: null,
      },
    });
    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(screen.getByTestId("admin-oxpilot-save")).toBeTruthy();
      expect(screen.getByText(/Required — the chat-completions endpoint/)).toBeTruthy();
    });

    fireEvent.submit(screen.getByTestId("admin-oxpilot-save").closest("form")!);
    await new Promise((r) => setTimeout(r, 300));
    expect(settingsSetMock).not.toHaveBeenCalled();

    // A base URL unblocks the write and goes through verbatim.
    fireEvent.input(screen.getByTestId("admin-oxpilot-base-url"), {
      target: { value: "http://litellm.local:4000" },
    });
    fireEvent.submit(screen.getByTestId("admin-oxpilot-save").closest("form")!);
    await waitFor(() => {
      expect(settingsSetMock).toHaveBeenCalledWith(
        expect.objectContaining({
          provider_id: "openai-compatible",
          base_url: "http://litellm.local:4000",
        }),
      );
    });
  });

  it("redirects signed-out sessions toward login", async () => {
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "Not signed in" },
    });

    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(assignMock).toHaveBeenCalledWith("/login?returnTo=/admin/oxpilot");
    });
  });

  it("shows forbidden for non sys-admin", async () => {
    meMock.mockResolvedValue({
      ok: true,
      data: { ...sysAdmin, role: "user" },
    });

    renderWithQueryClient(AdminOxpilotPage);

    await waitFor(() => {
      expect(screen.getByText(/You need admin access to manage OxPilot settings/i)).toBeTruthy();
    });
    expect(settingsGetMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("admin-oxpilot-page")).toBeNull();
  });
});
