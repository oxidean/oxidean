import { createElement } from "octane";
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BYTE_UNIT_FACTORS } from "@/lib/byte-units";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const getSettingsMock = vi.fn();
const getUsageMock = vi.fn();
const updateSettingsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      lfs: {
        getSettings: (...args: unknown[]) => getSettingsMock(...args),
        getUsage: (...args: unknown[]) => getUsageMock(...args),
        updateSettings: (...args: unknown[]) => updateSettingsMock(...args),
      },
    },
  },
}));

// @octanejs/recharts hits an octane 0.10 scoped-children bug
// (RegisterGraphicalItemId invokes a `children` render-prop that the runtime
// resolves before call). Stub the recharts primitives so the quota form tests
// exercise the real page without the dependency's chart internals.
vi.mock("@octanejs/recharts", () => {
  const stub = (name: string) => {
    const C = (_props: Record<string, unknown>) =>
      createElement("div", { "data-recharts-stub": name });
    return C;
  };
  return {
    BarChart: stub("BarChart"),
    Bar: stub("Bar"),
    CartesianGrid: stub("CartesianGrid"),
    XAxis: stub("XAxis"),
    YAxis: stub("YAxis"),
  };
});

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

const readySettings = {
  max_object_bytes: 2 * BYTE_UNIT_FACTORS.GiB,
  quota_repo_bytes: 10 * BYTE_UNIT_FACTORS.GiB,
  quota_user_bytes: 50 * BYTE_UNIT_FACTORS.GiB,
  max_object_bytes_overridden: false,
  quota_repo_bytes_overridden: true,
  quota_user_bytes_overridden: false,
};

const readyUsage = {
  object_count: 3,
  physical_bytes: 4 * BYTE_UNIT_FACTORS.GiB,
  logical_bytes: 5 * BYTE_UNIT_FACTORS.GiB,
  by_repo: [
    {
      repository_id: "r1",
      owner: "acme",
      name: "assets",
      object_count: 2,
      logical_bytes: 3 * BYTE_UNIT_FACTORS.GiB,
    },
    {
      repository_id: "r2",
      owner: "acme",
      name: "media",
      object_count: 1,
      logical_bytes: 2 * BYTE_UNIT_FACTORS.GiB,
    },
  ],
  by_owner: [
    {
      owner_id: "o1",
      owner_slug: "acme",
      object_count: 3,
      logical_bytes: 5 * BYTE_UNIT_FACTORS.GiB,
    },
  ],
};

import { AdminLfsPage } from "./lfs";

/**
 * D-LFS-12 / D-LFS-13 / D-LFS-19 + G-11.1-15: Admin LFS quotas must *render*
 * (raw-source checks alone missed missing useState / Octane template breakage).
 */
describe("admin LFS quotas (D-LFS-12 / D-LFS-13 / D-LFS-19)", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    meMock.mockReset();
    getSettingsMock.mockReset();
    getUsageMock.mockReset();
    updateSettingsMock.mockReset();
    meMock.mockResolvedValue({ ok: true, data: sysAdmin });
    getSettingsMock.mockResolvedValue({
      ok: true,
      data: readySettings,
    });
    getUsageMock.mockResolvedValue({
      ok: true,
      data: readyUsage,
    });
    updateSettingsMock.mockResolvedValue({
      ok: true,
      data: { ...readySettings, quota_repo_bytes_overridden: true },
    });
  });

  it("exports AdminLfsPage without @else if / bare Loading text and declares error/pending state", async () => {
    const mod = await import("./lfs");
    expect(mod.AdminLfsPage ?? mod.default).toBeTruthy();
    const src = await import("./lfs.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/getSettings/);
    expect(src).toMatch(/updateSettings/);
    expect(src).toMatch(/getUsage/);
    expect(src).toMatch(/fetchAdminLfsSettings|loader:/);
    expect(src).toMatch(/initialData/);
    expect(src).toMatch(/ByteQuotaField|@octanejs\/recharts|LfsUsageBarChart/);
    expect(src).not.toMatch(/AdminLfsSkeleton|showSkeleton/);
    expect(src).toMatch(/toastError|toastSuccess|toastWarning/);
    expect(src).toMatch(/kind: "error"|setPending|\[pending,/);
    expect(src).not.toMatch(/@else if/);
    expect(src).not.toMatch(/Loading…|Loading usage/);
  });

  it("renders quota amounts with unit selects and usage charts", async () => {
    renderWithQueryClient(AdminLfsPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("admin-lfs-page")).toBeTruthy();
        expect(screen.getByText("Git LFS quotas")).toBeTruthy();
        expect(screen.getByTestId("lfs-max-object-amount")).toBeTruthy();
        expect(screen.getByTestId("lfs-max-object-unit")).toBeTruthy();
        expect(screen.getByTestId("lfs-quota-repo-unit")).toBeTruthy();
        expect(screen.getByTestId("lfs-quota-user-unit")).toBeTruthy();
        expect(screen.getByLabelText("Max object size")).toBeTruthy();
        expect(screen.getByLabelText("Max object size unit")).toBeTruthy();
        expect(screen.getByText("Instance usage breakdown")).toBeTruthy();
        expect(screen.getByTestId("lfs-usage-chart-repo")).toBeTruthy();
        expect(screen.getByTestId("lfs-usage-chart-owner")).toBeTruthy();
      },
      { timeout: 10_000 },
    );

    const maxAmount = screen.getByTestId("lfs-max-object-amount") as HTMLInputElement;
    expect(maxAmount.value).toBe("2");
    expect(screen.getByText("Override active")).toBeTruthy();
    expect(screen.getAllByText(/acme\/assets/).length).toBeGreaterThan(0);
    await waitFor(
      () => {
        expect(screen.getByRole("img", { name: "By repository" })).toBeTruthy();
        expect(screen.getByRole("img", { name: "By owner" })).toBeTruthy();
      },
      { timeout: 10_000 },
    );
    expect(screen.queryByText("Loading…")).toBeNull();
  }, 20_000);

  it("submits quotas converted from display units into bytes", async () => {
    renderWithQueryClient(AdminLfsPage);

    await waitFor(() => {
      expect(screen.getByTestId("lfs-quota-repo-amount")).toBeTruthy();
    });

    const repoAmount = screen.getByTestId("lfs-quota-repo-amount") as HTMLInputElement;
    fireEvent.input(repoAmount, { target: { value: "12" } });

    fireEvent.click(screen.getByRole("button", { name: /Save quotas/i }));

    await waitFor(() => {
      expect(updateSettingsMock).toHaveBeenCalled();
    });

    const payload = updateSettingsMock.mock.calls[0]?.[0] as {
      max_object_bytes: number;
      quota_repo_bytes: number;
      quota_user_bytes: number;
    };
    expect(payload.max_object_bytes).toBe(2 * BYTE_UNIT_FACTORS.GiB);
    expect(payload.quota_repo_bytes).toBe(12 * BYTE_UNIT_FACTORS.GiB);
    expect(payload.quota_user_bytes).toBe(50 * BYTE_UNIT_FACTORS.GiB);
  });
});
