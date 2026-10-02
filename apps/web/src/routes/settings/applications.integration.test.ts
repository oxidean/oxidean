import { createElement } from "octane";
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

/**
 * API-03 /settings/applications — developer apps + authorized grants UI.
 */

const listMock = vi.fn();
const grantsMock = vi.fn();
const createMock = vi.fn();
const updateMock = vi.fn();
const deleteMock = vi.fn();
const rotateMock = vi.fn();
const revokeMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    oauthApp: {
      list: (...args: unknown[]) => listMock(...args),
      listGrants: (...args: unknown[]) => grantsMock(...args),
      create: (...args: unknown[]) => createMock(...args),
      update: (...args: unknown[]) => updateMock(...args),
      delete: (...args: unknown[]) => deleteMock(...args),
      regenerateSecret: (...args: unknown[]) => rotateMock(...args),
      revoke: (...args: unknown[]) => revokeMock(...args),
    },
  },
}));

type LoaderShape =
  | { kind: "unauthenticated" }
  | { kind: "error"; message: string }
  | { kind: "ready"; user: unknown; apps: unknown[]; grants: unknown[] };

let loaderData: LoaderShape;

vi.mock("@octanejs/tanstack-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@octanejs/tanstack-router")>();
  return {
    ...actual,
    useLoaderData: () => loaderData,
    Link: (props: {
      to?: string;
      children?: unknown;
      className?: string;
      "aria-current"?: string;
    }) =>
      createElement(
        "a",
        {
          href: props.to ?? "#",
          className: props.className,
          "aria-current": props["aria-current"],
        },
        props.children as never,
      ),
  };
});

const verifiedUser = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
};

const appRow = {
  id: "app-1",
  name: "test-cli",
  client_id: "oxidean_oc_0123456789abcdef0123456789abcdef",
  client_secret_prefix: "oxidean_osec_ab12cd34",
  redirect_uris: ["https://app.example/callback"],
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
};

const grantRow = {
  application_id: "app-1",
  app_name: "test-cli",
  client_id: appRow.client_id,
  scopes: ["repo", "read:user"],
  granted_at: new Date().toISOString(),
  last_used_at: null,
};

beforeEach(() => {
  listMock.mockReset();
  grantsMock.mockReset();
  createMock.mockReset();
  updateMock.mockReset();
  deleteMock.mockReset();
  rotateMock.mockReset();
  revokeMock.mockReset();
  loaderData = { kind: "ready", user: verifiedUser, apps: [appRow], grants: [grantRow] };
  listMock.mockResolvedValue({ ok: true, data: [appRow] });
  grantsMock.mockResolvedValue({ ok: true, data: [grantRow] });
});

afterEach(cleanup);

async function loadModule(): Promise<Record<string, unknown>> {
  return (await import("./applications")) as Record<string, unknown>;
}

describe("/settings/applications (API-03)", () => {
  it("renders authorized grants + owned apps", async () => {
    const mod = await loadModule();
    const Page = (mod.ApplicationsPage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(document.body.textContent).toContain("OAuth applications");
    });
    expect(document.body.textContent).toContain("Authorized applications");
    expect(document.body.textContent).toContain("Developer applications");
    expect(document.body.textContent).toContain("test-cli");
    expect(document.body.textContent).toContain(appRow.client_id);
    expect(screen.getAllByRole("button", { name: /Revoke/i }).length).toBeGreaterThan(0);
    expect(
      screen.getAllByRole("button", { name: /Register new application/i })[0],
    ).toBeInTheDocument();
  }, 30_000);

  it("empty state when no apps or grants", async () => {
    loaderData = { kind: "ready", user: verifiedUser, apps: [], grants: [] };
    listMock.mockResolvedValue({ ok: true, data: [] });
    const mod = await loadModule();
    const Page = (mod.ApplicationsPage ?? mod.default) as unknown;
    renderWithQueryClient(Page);

    await waitFor(() => {
      expect(document.body.textContent).toContain("No authorized applications");
    });
    expect(document.body.textContent).toContain("No OAuth applications");
  }, 30_000);
});
