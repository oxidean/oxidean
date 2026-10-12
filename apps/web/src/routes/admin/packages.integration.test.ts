/**
 * Admin packages quota/usage (D-PKG-09) + G-11.1-15 render mount.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { DEFAULT_OWNER_QUOTA_HINT, DEFAULT_OWNER_QUOTA_LABEL } from "@/lib/package-quota-copy";

const meMock = vi.fn();
const adminUsageMock = vi.fn();
const adminSetQuotaMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    packages: {
      adminUsage: (...args: unknown[]) => adminUsageMock(...args),
      adminSetQuota: (...args: unknown[]) => adminSetQuotaMock(...args),
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

import { AdminPackagesPage } from "./packages";

afterEach(cleanup);

beforeEach(() => {
  meMock.mockResolvedValue({ ok: true, data: sysAdmin });
  adminUsageMock.mockReset();
  adminSetQuotaMock.mockReset();
});

describe("/admin/packages", () => {
  it("exports AdminPackagesPage and keeps owner-quota copy hints", () => {
    expect(typeof AdminPackagesPage).toBe("function");
    expect(DEFAULT_OWNER_QUOTA_HINT.toLowerCase()).toContain("quota");
    expect(DEFAULT_OWNER_QUOTA_HINT).toContain("OXIDEAN_PACKAGES_OWNER_QUOTA_BYTES");
    expect(DEFAULT_OWNER_QUOTA_LABEL).toMatch(/GiB/);
  });

  it("renders Package storage for admin without throwing (G-11.1-15)", async () => {
    renderWithQueryClient(AdminPackagesPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-packages")).toBeTruthy();
        expect(screen.getByRole("heading", { name: "Package storage" })).toBeTruthy();
        expect(screen.getByText("Instance defaults")).toBeTruthy();
        expect(screen.getByText("Per-owner quota")).toBeTruthy();
        expect(screen.getByText("Owner usage & quota")).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  }, 15_000);

  it("shows forbidden for non sys-admin from loader", async () => {
    meMock.mockResolvedValue({ ok: true, data: { ...sysAdmin, role: "user" } });

    renderWithQueryClient(AdminPackagesPage);

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/admin access/i);
    });
  });

  it("loads owner usage after Show usage", async () => {
    adminUsageMock.mockResolvedValue({
      ok: true,
      data: {
        owner_type: "user",
        owner_id: "u2",
        used_bytes: 1024,
        quota_bytes: 10 * 1024 * 1024 * 1024,
        default_quota_bytes: 10 * 1024 * 1024 * 1024,
        by_format: [{ format: "npm", bytes: 1024 }],
        packages: [{ package_id: "p1", name: "left-pad", format: "npm", bytes: 1024 }],
      },
    });

    renderWithQueryClient(AdminPackagesPage);

    await waitFor(() => expect(screen.getByTestId("admin-packages-owner")).toBeTruthy());
    fireEvent.input(screen.getByTestId("admin-packages-owner"), {
      target: { value: "alice" },
    });
    fireEvent.click(screen.getByTestId("admin-packages-lookup"));

    await waitFor(() => {
      expect(adminUsageMock).toHaveBeenCalled();
      expect(screen.getByTestId("admin-packages-usage")).toBeTruthy();
      expect(screen.getByText("Instance default")).toBeTruthy();
      expect(screen.getByText("left-pad")).toBeTruthy();
      expect(screen.getByTestId("admin-packages-list").textContent).toMatch(/npm/);
    });
  });
});
