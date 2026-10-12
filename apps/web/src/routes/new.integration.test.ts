import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { trackDomErrors } from "@/test/dom-errors";

/**
 * /new create flow (D-01, D-02, D-04, D-11, D-12 / UI-SPEC).
 */

const createMock = vi.fn();

vi.mock("@/lib/spdx-licenses", () => ({
  listLicensePickerOptions: () => [
    {
      id: "MIT",
      label: "MIT",
      group: "Popular",
      description: "Permissive — keep the copyright notice.",
    },
    {
      id: "Apache-2.0",
      label: "Apache 2.0",
      group: "Popular",
      description: "Permissive with an express patent grant.",
    },
  ],
  listSpdxLicenseOptions: () => [
    { id: "none", label: "None" },
    { id: "MIT", label: "MIT — MIT License" },
  ],
}));

const meMock = vi.fn();
const createDefaultsMock = vi.fn();
const listMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    repo: {
      create: (...args: unknown[]) => createMock(...args),
      createDefaults: (...args: unknown[]) => createDefaultsMock(...args),
    },
    org: {
      listMine: (...args: unknown[]) => listMineMock(...args),
    },
  },
}));

beforeEach(() => {
  window.history.pushState({}, "", "/new");
  createMock.mockReset();
  meMock.mockReset();
  createDefaultsMock.mockReset();
  listMineMock.mockReset();
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
      email_verified: false,
    },
    defaults: null,
    ownerOrgs: [],
  };
  meMock.mockResolvedValue({ ok: true, data: __ld.user });
  createDefaultsMock.mockResolvedValue(
    __ld.defaults
      ? { ok: true, data: __ld.defaults }
      : { ok: false, error: { code: "x", message: "x" } },
  );
  listMineMock.mockResolvedValue({ ok: true, data: { orgs: __ld.ownerOrgs ?? [] } });
});

afterEach(cleanup);

describe("/new Wave 0 (D-11 verify wall)", () => {
  it("unverified session shows Verify your email wall — not the create form", async () => {
    const { NewPage } = await import("./new");
    render(NewPage as never);

    await waitFor(() => {
      expect(screen.getByText("Verify your email")).toBeInTheDocument();
    });
    expect(screen.getByText("Verify your email before creating a repository.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Verify email" })).toHaveAttribute("href", "/verify");
    expect(screen.queryByRole("button", { name: "Create repository" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/repository name/i)).not.toBeInTheDocument();
  }, 20000);
});

describe("/new create form (D-02, D-04, D-12)", () => {
  it("verified form shows stack, license, gitignore pickers", async () => {
    const tracker = trackDomErrors();
    try {
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
          stacks: [
            {
              id: "rust",
              label: "Rust",
              group: "Systems",
              description: "Cargo binary crate with src/main.rs.",
              default_gitignore: "Rust",
              provenance: "builtin",
            },
            {
              id: "go",
              label: "Go",
              group: "Backend",
              description: "Go module.",
              default_gitignore: "Go",
              provenance: "builtin",
            },
            {
              id: "nextjs",
              label: "Next.js",
              group: "Frontend",
              description: "Next app router.",
              default_gitignore: "Node",
              provenance: "builtin",
            },
          ],
          gitignores: [
            {
              id: "Rust",
              label: "Rust",
              group: "Languages",
              description: "target/ and Cargo build noise.",
            },
            {
              id: "Go",
              label: "Go",
              group: "Languages",
              description: "bin/",
            },
            {
              id: "Node",
              label: "Node",
              group: "Languages",
              description: "node_modules/",
            },
          ],
        },
        ownerOrgs: [],
      };
      meMock.mockResolvedValue({ ok: true, data: __ld.user });
      createDefaultsMock.mockResolvedValue(
        __ld.defaults
          ? { ok: true, data: __ld.defaults }
          : { ok: false, error: { code: "x", message: "x" } },
      );
      listMineMock.mockResolvedValue({ ok: true, data: { orgs: __ld.ownerOrgs ?? [] } });

      const { NewPage } = await import("./new");
      render(NewPage as never);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: "Create repository" })).toBeInTheDocument();
      });
      expect(screen.getByLabelText("Stack / template")).toBeInTheDocument();
      expect(screen.getByLabelText("License")).toBeInTheDocument();
      expect(screen.getByLabelText(".gitignore")).toBeInTheDocument();

      const stackDetails = screen.getByLabelText("Stack / template").closest("details");
      if (!stackDetails) throw new Error("expected stack <details>");
      stackDetails.open = true;
      fireEvent(stackDetails, new Event("toggle", { bubbles: true }));
      await waitFor(() => {
        expect(screen.getByTestId("repo-stack-overlay")).toBeVisible();
        expect(
          screen.getByRole("heading", { name: "Choose Stack / template", hidden: true }),
        ).toBeInTheDocument();
      });
      expect(screen.getByText(/Cargo binary crate with src\/main\.rs\./)).toBeInTheDocument();
      // Non-first group + sibling gitignore autofill — regression for insertBefore races.
      fireEvent.click(screen.getByRole("button", { name: /^Next\.js/, hidden: true }));
      await waitFor(() => {
        expect(stackDetails.open).toBe(false);
      });
      expect(screen.getByLabelText(".gitignore")).toHaveTextContent(/Node/);
      expect(tracker.domRaceErrors()).toEqual([]);

      const licenseDetails = screen.getByLabelText("License").closest("details");
      if (!licenseDetails) throw new Error("expected license <details>");
      licenseDetails.open = true;
      fireEvent(licenseDetails, new Event("toggle", { bubbles: true }));
      await waitFor(() => {
        expect(screen.getByTestId("repo-license-overlay")).toBeVisible();
        expect(
          screen.getByRole("heading", { name: "Choose License", hidden: true }),
        ).toBeInTheDocument();
      });
      expect(screen.getByText("Permissive — keep the copyright notice.")).toBeInTheDocument();
      expect(tracker.domRaceErrors()).toEqual([]);
    } finally {
      tracker.dispose();
    }
  }, 20000);

  it("duplicate name maps to exact inline field copy", async () => {
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
      ownerOrgs: [],
    };
    meMock.mockResolvedValue({ ok: true, data: __ld.user });
    createDefaultsMock.mockResolvedValue(
      __ld.defaults
        ? { ok: true, data: __ld.defaults }
        : { ok: false, error: { code: "x", message: "x" } },
    );
    listMineMock.mockResolvedValue({ ok: true, data: { orgs: __ld.ownerOrgs ?? [] } });

    createMock.mockResolvedValueOnce({
      ok: false,
      error: {
        code: "repo.name_taken",
        message: "ignored server wording",
      },
    });

    const { NewPage } = await import("./new");
    render(NewPage as never);

    await waitFor(() => {
      expect(screen.getByLabelText(/repository name/i)).toBeInTheDocument();
    });

    const input = screen.getByLabelText(/repository name/i);
    fireEvent.input(input, { target: { value: "dup-repo" } });
    fireEvent.click(screen.getByRole("button", { name: "Create repository" }));

    await waitFor(() => {
      expect(
        screen.getByText("A repository with this name already exists. Choose a different name."),
      ).toBeInTheDocument();
    });
  }, 20000);
});
