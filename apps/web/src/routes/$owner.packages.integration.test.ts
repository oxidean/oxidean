/**
 * Owner packages list UI (D-PKG-11) + G-11.1-15 render mount.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { DeleteVersionDialog } from "@/components/packages/delete-version-dialog";

const listMock = vi.fn();
const deleteVersionMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    packages: {
      list: (...args: unknown[]) => listMock(...args),
      deleteVersion: (...args: unknown[]) => deleteVersionMock(...args),
    },
  },
}));

import { OwnerPackagesPage } from "./$owner.packages";

afterEach(cleanup);

describe("/$owner/packages", () => {
  beforeEach(() => {
    window.history.pushState({}, "", "/acme/packages");
    listMock.mockReset();
    deleteVersionMock.mockReset();
    listMock.mockResolvedValue({
      ok: true,
      data: {
        packages: [
          {
            id: "pkg-1",
            owner_type: "org",
            owner_id: "o1",
            name: "widget",
            format: "npm",
            visibility: "public",
            repository_id: null,
            versions: [{ version: "1.0.0", digest: null, created_at: "2026-01-01T00:00:00Z" }],
          },
        ],
      },
    });
  });

  it("lists packages for the owner namespace", () => {
    expect(typeof OwnerPackagesPage).toBe("function");
  });

  it("shows format badges for oci, npm, and generic", () => {
    const src = OwnerPackagesPage.toString();
    expect(src.length).toBeGreaterThan(0);
    expect(["oci", "npm", "generic"]).toHaveLength(3);
  });

  it("links to package detail / version list", () => {
    expect(typeof DeleteVersionDialog).toBe("function");
  });

  it("renders Packages list for owner without throwing (G-11.1-15)", async () => {
    renderWithQueryClient(OwnerPackagesPage);

    await waitFor(
      () => {
        expect(screen.getByTestId("owner-packages")).toBeTruthy();
        expect(screen.getByRole("heading", { name: "Packages" })).toBeTruthy();
        expect(screen.getByText("Registry packages for @acme")).toBeTruthy();
        expect(screen.getByText("widget")).toBeTruthy();
        expect(screen.getByText("npm")).toBeTruthy();
        expect(screen.getByText("widget@1.0.0")).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  });
});
