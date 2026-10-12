/**
 * Admin instance templates (issue #18) + G-11.1-15 render mount.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const listMock = vi.fn();
const createDefaultsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    admin: {
      templates: {
        list: (...args: unknown[]) => listMock(...args),
        setEnabled: vi.fn(),
        delete: vi.fn(),
        update: vi.fn(),
      },
    },
    repo: {
      createDefaults: (...args: unknown[]) => createDefaultsMock(...args),
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

import { AdminTemplatesPage } from "./templates";

afterEach(cleanup);

beforeEach(() => {
  meMock.mockResolvedValue({ ok: true, data: sysAdmin });
  listMock.mockReset();
  createDefaultsMock.mockReset();
  listMock.mockResolvedValue({
    ok: true,
    data: {
      packs: [
        {
          id: "p1",
          slug: "acme-node",
          label: "Acme Node",
          group: "Custom",
          description: "Instance starter",
          enabled: true,
          byte_size: 128,
          content_digest: "sha256:abc",
          uploaded_by_user_id: "u1",
          created_at: "2026-01-01T00:00:00Z",
          updated_at: "2026-01-01T00:00:00Z",
        },
      ],
    },
  });
  createDefaultsMock.mockResolvedValue({
    ok: true,
    data: {
      default_visibility: "public",
      stacks: [
        {
          id: "rust",
          label: "Rust",
          group: "Systems",
          description: "Cargo binary",
          provenance: "builtin",
          default_gitignore: "Rust",
        },
      ],
      gitignores: [
        {
          id: "Rust",
          label: "Rust",
          group: "Languages",
          description: "target/",
        },
      ],
    },
  });
});

describe("/admin/templates", () => {
  it("exports AdminTemplatesPage", () => {
    expect(typeof AdminTemplatesPage).toBe("function");
  });

  it("renders official and instance packs for admin (G-11.1-15)", async () => {
    renderWithQueryClient(AdminTemplatesPage);

    await waitFor(
      () => {
        expect(screen.getByRole("heading", { name: "Templates" })).toBeTruthy();
        expect(screen.getByRole("heading", { name: "Official packs" })).toBeTruthy();
        expect(screen.getByRole("heading", { name: "Instance packs" })).toBeTruthy();
        expect(screen.getByText("Rust")).toBeTruthy();
        expect(screen.getByText("Acme Node")).toBeTruthy();
        expect(screen.getByLabelText("Slug")).toBeTruthy();
        expect(screen.getByLabelText("Group")).toBeTruthy();
        expect(screen.getByLabelText("Default .gitignore")).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  }, 15_000);

  it("shows forbidden for non sys-admin from loader", async () => {
    meMock.mockResolvedValue({ ok: true, data: { ...sysAdmin, role: "user" } });

    renderWithQueryClient(AdminTemplatesPage);

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/admin access/i);
    });
  });
});
