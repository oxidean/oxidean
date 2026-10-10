import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const getSettingsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      auth: {
        getSettings: (...args: unknown[]) => getSettingsMock(...args),
        updateSettings: vi.fn(),
        factoryReset: vi.fn(),
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

const settings = {
  provider_mode: "local" as const,
  email_provider: "log" as const,
  from_address: null,
  workos_client_id: null,
  oidc_issuer: null,
  oidc_client_id: null,
  smtp_configured: false,
  resend_configured: false,
  workos_api_key_configured: false,
  oidc_client_secret_configured: false,
  allow_signup: false,
  default_visibility: "public" as const,
};

import { AdminAuthPage } from "./auth";

describe("/admin/auth SSR-backed settings", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    meMock.mockReset();
    getSettingsMock.mockReset();
    meMock.mockResolvedValue({ ok: true, data: sysAdmin });
    getSettingsMock.mockResolvedValue({ ok: true, data: settings });
  });

  it("declares SSR loader and seeds form without AdminAuthSkeleton", async () => {
    const src = await import("./auth.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/fetchAdminAuthSettings|loader:/);
    expect(src).toMatch(/initialData/);
    expect(src).not.toMatch(/AdminAuthSkeleton/);
    expect(src).not.toMatch(/@else if/);
  });

  it("shows Allow open signup Switch with wizard helper copy", async () => {
    renderWithQueryClient(AdminAuthPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-auth-page")).toBeTruthy();
        expect(screen.getByText("Auth settings")).toBeInTheDocument();
      },
      { timeout: 3000 },
    );

    expect(screen.getByText("Allow open signup")).toBeInTheDocument();
    expect(screen.getByText(/When off, new accounts can’t self-register/i)).toBeInTheDocument();
    expect(screen.getByText("Danger zone")).toBeInTheDocument();
  }, 10000);

  it("shows forbidden for non sys-admin from loader", async () => {
    meMock.mockResolvedValue({ ok: true, data: { ...sysAdmin, role: "user" } });

    meMock.mockResolvedValue({
      ok: true,
      data: { ...sysAdmin, role: "user" },
    });

    renderWithQueryClient(AdminAuthPage);

    await waitFor(() => {
      expect(
        screen.getByText(/You need admin access to manage auth settings/i),
      ).toBeInTheDocument();
    });
    expect(getSettingsMock).not.toHaveBeenCalled();
  });

  it("updates Email delivery Select and shows SMTP credentials section", async () => {
    const src = await import("./auth.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/form\.Subscribe/);

    renderWithQueryClient(AdminAuthPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-auth-page")).toBeTruthy();
        expect(screen.getByLabelText(/^Email delivery$/i)).toBeInTheDocument();
      },
      { timeout: 3000 },
    );

    expect(screen.queryByText(/OXIDEAN_SMTP_URL/i)).not.toBeInTheDocument();

    const emailTrigger = screen.getByLabelText(/^Email delivery$/i);
    expect(emailTrigger).toHaveTextContent(/Log sink/i);

    fireEvent.click(emailTrigger);
    await waitFor(() => {
      expect(screen.getByRole("option", { name: /^SMTP$/i })).toBeInTheDocument();
    });
    const smtpOption = screen.getByRole("option", { name: /^SMTP$/i });
    // Base UI SelectItem only commits after pointerdown sets allowMouseSelectionRef.
    fireEvent.pointerDown(smtpOption, { pointerType: "mouse" });
    fireEvent.click(smtpOption);

    await waitFor(() => {
      expect(screen.getByLabelText(/^Email delivery$/i)).toHaveTextContent(/SMTP/i);
      expect(screen.getByText(/SMTP credentials/i)).toBeInTheDocument();
      expect(
        screen.getByText(/configure OXIDEAN_SMTP_URL in the environment/i),
      ).toBeInTheDocument();
    });
  }, 15_000);
});
