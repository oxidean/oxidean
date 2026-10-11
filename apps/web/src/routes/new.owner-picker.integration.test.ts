import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * D-ORG-06: /new owner picker (self + Owner/Admin orgs).
 */

const createMock = vi.fn();
const listMineMock = vi.fn();

vi.mock("@/lib/spdx-licenses", () => ({
  listLicensePickerOptions: () => [],
  listSpdxLicenseOptions: () => [{ id: "none", label: "None" }],
}));

const meMock = vi.fn();
const createDefaultsMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    org: {
      listMine: (...args: unknown[]) => listMineMock(...args),
    },
    repo: {
      create: (...args: unknown[]) => createMock(...args),
      createDefaults: (...args: unknown[]) => createDefaultsMock(...args),
    },
  },
}));

beforeEach(() => {
  window.history.pushState({}, "", "/new");
  createMock.mockReset();
  meMock.mockReset();
  createDefaultsMock.mockReset();
  listMineMock.mockReset();
  listMineMock.mockResolvedValue({
    ok: true,
    data: {
      orgs: [
        { slug: "acme", display_name: "Acme", role: "owner" },
        { slug: "widgets", display_name: "Widgets", role: "admin" },
        { slug: "readonly-co", display_name: "ReadOnly Co", role: "member" },
      ],
    },
  });
  const __ld = {
    user: {
      id: "u1",
      email: "ada@example.com",
      username: "ada",
      display_name: "Ada",
      bio: "",
      avatar_url: null,
      role: "user",
      profile_incomplete: false,
      email_verified: true,
    },
    defaults: {
      default_visibility: "public",
      stacks: [],
      gitignores: [],
    },
    // SSR already filters to Owner/Admin — Member-only orgs omitted (D-ORG-06).
    ownerOrgs: [
      {
        id: "o1",
        slug: "acme",
        display_name: "Acme",
        member_base_permission: "none",
        role: "owner",
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      },
      {
        id: "o2",
        slug: "widgets",
        display_name: "Widgets",
        member_base_permission: "none",
        role: "admin",
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      },
    ],
  };
  meMock.mockResolvedValue({ ok: true, data: __ld.user });
  createDefaultsMock.mockResolvedValue(
    __ld.defaults
      ? { ok: true, data: __ld.defaults }
      : { ok: false, error: { code: "x", message: "x" } },
  );
  listMineMock.mockResolvedValue({ ok: true, data: { orgs: __ld.ownerOrgs ?? [] } });
});

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

import { NewPage } from "./new";

function ownerTrigger(): Promise<HTMLElement> {
  return screen.findByLabelText(/^Owner$/i, {}, { timeout: 10_000 });
}

describe("/new owner picker (D-ORG-06)", () => {
  it("lists @self + Owner/Admin orgs — not Member-only orgs", async () => {
    render(NewPage as never);

    const ownerControl = await ownerTrigger();
    expect(
      ownerControl,
      "Owner Select/combobox listing self + Owner/Admin orgs (D-ORG-06)",
    ).toBeTruthy();

    fireEvent.click(ownerControl);
    await waitFor(() => {
      expect(screen.getByRole("option", { name: "@ada" })).toBeInTheDocument();
      expect(screen.getByRole("option", { name: /Acme/ })).toBeInTheDocument();
      expect(screen.getByRole("option", { name: /Widgets/ })).toBeInTheDocument();
    });
    expect(screen.queryByRole("option", { name: /ReadOnly Co/i })).not.toBeInTheDocument();
  }, 20_000);

  it("removes Organizations come in a later phase copy", async () => {
    render(NewPage as never);

    expect(screen.queryByText(/Organizations come in a later phase/i)).not.toBeInTheDocument();
  }, 20_000);

  it("repo.create posts selected owner slug", async () => {
    createMock.mockResolvedValue({
      ok: true,
      data: {
        id: "r1",
        owner_id: "u1",
        owner_type: "user",
        owner_username: "ada",
        name: "demo",
        description: "",
        visibility: "public",
        default_branch: "main",
        updated_at: "2026-01-01T00:00:00Z",
        can_admin: true,
        can_write: true,
      },
    });

    render(NewPage as never);

    // Default selection is self; owner slug must still be posted (D-ORG-06).
    expect(await ownerTrigger()).toHaveTextContent("@ada");

    fireEvent.input(screen.getByLabelText(/repository name/i), {
      target: { value: "demo" },
    });
    fireEvent.click(screen.getByRole("button", { name: /create repository/i }));

    await waitFor(() => {
      expect(createMock).toHaveBeenCalled();
    });
    const payload = createMock.mock.calls[0]?.[0] as { owner?: string };
    expect(payload.owner).toBe("ada");
  }, 20_000);
});
