/**
 * Admin instance templates (issue #18) + G-11.1-15 render mount.
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const listMock = vi.fn();
const createDefaultsMock = vi.fn();
const wfListMock = vi.fn();
const wfCreateMock = vi.fn();
const wfUpdateMock = vi.fn();
const wfSetEnabledMock = vi.fn();
const wfDeleteMock = vi.fn();

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
      workflowTemplates: {
        list: (...args: unknown[]) => wfListMock(...args),
        create: (...args: unknown[]) => wfCreateMock(...args),
        update: (...args: unknown[]) => wfUpdateMock(...args),
        setEnabled: (...args: unknown[]) => wfSetEnabledMock(...args),
        delete: (...args: unknown[]) => wfDeleteMock(...args),
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

const WF_TEMPLATE = {
  id: "wt-1",
  slug: "acme-ci",
  name: "Acme CI",
  description: "Instance CI starter",
  icon_name: "rust",
  categories: ["Continuous integration"],
  file_patterns: ["Cargo.toml"],
  filename: "acme-ci.yml",
  enabled: true,
  content: "name: acme-ci\non: [push]\njobs:\n  build:\n    runs-on: oxidean\n    steps: []\n",
  created_by_user_id: "u1",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

beforeEach(() => {
  meMock.mockResolvedValue({ ok: true, data: sysAdmin });
  listMock.mockReset();
  createDefaultsMock.mockReset();
  wfListMock.mockReset();
  wfCreateMock.mockReset();
  wfUpdateMock.mockReset();
  wfSetEnabledMock.mockReset();
  wfDeleteMock.mockReset();
  wfListMock.mockResolvedValue({ ok: true, data: { templates: [WF_TEMPLATE] } });
  wfCreateMock.mockResolvedValue({ ok: true, data: WF_TEMPLATE });
  wfUpdateMock.mockResolvedValue({ ok: true, data: WF_TEMPLATE });
  wfSetEnabledMock.mockResolvedValue({ ok: true, data: {} });
  wfDeleteMock.mockResolvedValue({ ok: true, data: {} });
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

  it("lists workflow templates and creates one via the form", async () => {
    renderWithQueryClient(AdminTemplatesPage);

    await waitFor(() => expect(screen.getByTestId("admin-wf-row-wt-1")).toBeTruthy());
    expect(screen.getByTestId("admin-wf-row-wt-1").textContent).toContain("Acme CI");

    screen.getByTestId("admin-wf-new").click();
    await waitFor(() => expect(screen.getByTestId("admin-wf-form")).toBeTruthy());

    const setValue = (testid: string, value: string) => {
      const el = screen.getByTestId(testid) as HTMLInputElement | HTMLTextAreaElement;
      el.value = value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
    };
    setValue("admin-wf-slug", "acme-ci");
    setValue("admin-wf-name", "Acme CI");
    setValue("admin-wf-desc", "Instance CI starter");
    setValue("admin-wf-content", WF_TEMPLATE.content);
    screen.getByTestId("admin-wf-save").click();

    await waitFor(() => {
      expect(wfCreateMock).toHaveBeenCalledWith(
        expect.objectContaining({
          slug: "acme-ci",
          name: "Acme CI",
          filename: "acme-ci.yml",
          content: WF_TEMPLATE.content,
        }),
      );
    });
  }, 15_000);

  it("surfaces invalid-workflow errors inline", async () => {
    wfCreateMock.mockResolvedValue({
      ok: false,
      error: {
        code: "actions.invalid_workflow",
        message: "jobs.build: missing runs-on",
      },
    });

    renderWithQueryClient(AdminTemplatesPage);
    await waitFor(() => expect(screen.getByTestId("admin-wf-row-wt-1")).toBeTruthy());

    screen.getByTestId("admin-wf-new").click();
    await waitFor(() => expect(screen.getByTestId("admin-wf-form")).toBeTruthy());

    const setValue = (testid: string, value: string) => {
      const el = screen.getByTestId(testid) as HTMLInputElement | HTMLTextAreaElement;
      el.value = value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
    };
    setValue("admin-wf-slug", "bad-ci");
    setValue("admin-wf-name", "Bad CI");
    setValue("admin-wf-content", "name: bad\non: [push]\n");
    screen.getByTestId("admin-wf-save").click();

    await waitFor(() => {
      expect(screen.getByTestId("admin-wf-error").textContent).toContain("missing runs-on");
    });
  }, 15_000);
});
