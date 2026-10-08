import { createElement } from "octane";
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

vi.mock("@octanejs/tanstack-router", () => ({
  Link: (props: { to?: string; children?: unknown; className?: string; onClick?: () => void }) =>
    createElement(
      "a",
      {
        href: props.to ?? "#",
        className: props.className,
        onClick: props.onClick,
      },
      props.children as never,
    ),
  useNavigate: () => () => undefined,
  useParams: () => ({}),
}));

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: vi.fn(async () => ({
        ok: false,
        error: { code: "auth.unauthenticated", message: "n" },
      })),
      bootstrapStatus: vi.fn(async () => ({
        ok: true,
        data: { needs_setup: false },
      })),
      providerConfig: vi.fn(async () => ({
        ok: true,
        data: { mode: "local", allow_signup: false },
      })),
      logout: vi.fn(async () => ({ ok: true, data: { ok: true } })),
    },
    org: {
      listMine: vi.fn(async () => ({ ok: true, data: { orgs: [] } })),
    },
  },
}));

import { apiClient } from "@/lib/api-client";
import { SiteHeader } from "./chrome";

afterEach(cleanup);

beforeEach(() => {
  vi.mocked(apiClient.auth.me).mockResolvedValue({
    ok: false,
    error: { code: "auth.unauthenticated", message: "n" },
  } as never);
  vi.mocked(apiClient.auth.bootstrapStatus).mockResolvedValue({
    ok: true,
    data: { needs_setup: false },
  } as never);
  vi.mocked(apiClient.auth.providerConfig).mockResolvedValue({
    ok: true,
    data: { mode: "local", allow_signup: false },
  } as never);
});

describe("chrome pending account cluster", () => {
  it("emits both skeleton candidates while auth.me is pending", async () => {
    let resolveMe: (v: unknown) => void = () => {};
    vi.mocked(apiClient.auth.me).mockReturnValueOnce(
      new Promise((res) => {
        resolveMe = res;
      }) as never,
    );

    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(document.querySelector("[data-header-skeleton-anon]")).toBeTruthy();
      expect(document.querySelector("[data-header-skeleton-authed]")).toBeTruthy();
    });
    expect(document.querySelector('[role="group"][aria-busy="true"]')).toBeTruthy();

    resolveMe({ ok: false, error: { code: "auth.unauthenticated", message: "n" } });
    await waitFor(() => {
      expect(screen.getByRole("link", { name: /sign in/i })).toBeInTheDocument();
    });
    expect(document.querySelector("[data-header-skeleton-anon]")).toBeNull();
    expect(document.querySelector("[data-header-skeleton-authed]")).toBeNull();
  });
});

describe("chrome Wave 0 (D-06 omit Sign up)", () => {
  it("omits Sign up when allow_signup is false", async () => {
    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /sign in/i })).toBeInTheDocument();
    });

    expect(screen.queryByRole("link", { name: /^sign up$/i })).not.toBeInTheDocument();
  });

  it("omits Sign up when allow_signup is unknown", async () => {
    vi.mocked(apiClient.auth.providerConfig).mockResolvedValueOnce({
      ok: true,
      data: { mode: "local" },
    } as never);

    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /sign in/i })).toBeInTheDocument();
    });

    expect(screen.queryByRole("link", { name: /^sign up$/i })).not.toBeInTheDocument();
  });

  it("omits Sign in and Sign up while needs_setup", async () => {
    vi.mocked(apiClient.auth.bootstrapStatus).mockResolvedValueOnce({
      ok: true,
      data: { needs_setup: true },
    } as never);

    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(screen.queryByLabelText(/^Account$/i)).toBeTruthy();
    });

    expect(screen.queryByRole("link", { name: /sign in/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /^sign up$/i })).not.toBeInTheDocument();
  });

  it("shows Sign up when allow_signup is true", async () => {
    vi.mocked(apiClient.auth.providerConfig).mockResolvedValueOnce({
      ok: true,
      data: { mode: "local", allow_signup: true },
    } as never);

    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /^sign up$/i })).toBeInTheDocument();
    });

    expect(screen.getByRole("link", { name: /sign in/i })).toBeInTheDocument();
  });

  it("uses a shared QueryClient path for auth.me (not N independent effects)", async () => {
    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /sign in/i })).toBeInTheDocument();
    });

    // Desktop header only mounts AccountActions once. Allow ≤2 for act/strict remount;
    // the pre-Query chrome pattern still used one effect per mount (also ≤2 under act).
    expect(apiClient.auth.me.mock.calls.length).toBeLessThanOrEqual(2);
    expect(apiClient.auth.bootstrapStatus.mock.calls.length).toBeLessThanOrEqual(2);
    expect(apiClient.auth.providerConfig.mock.calls.length).toBeLessThanOrEqual(2);
  });
});

describe("chrome signed-in create + account menus", () => {
  const signedInUser = {
    id: "u1",
    email: "ada@example.com",
    username: "ada",
    display_name: "Ada",
    bio: "",
    avatar_url: null,
    role: "user" as const,
    profile_incomplete: false,
    email_verified: true,
    must_change_credentials: false,
    default_branch: "main",
  };

  beforeEach(() => {
    vi.mocked(apiClient.auth.me).mockResolvedValue({
      ok: true,
      data: signedInUser,
    } as never);
    vi.mocked(apiClient.org.listMine).mockResolvedValue({
      ok: true,
      data: {
        orgs: [
          {
            id: "o1",
            slug: "acme",
            display_name: "Acme",
            member_base_permission: "read",
            role: "owner",
            created_at: "2026-01-01T00:00:00Z",
            updated_at: "2026-01-01T00:00:00Z",
          },
        ],
      },
    } as never);
  });

  it("shows Create new and Account menu triggers when signed in", async () => {
    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /create new/i })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /account menu/i })).toBeInTheDocument();
    });

    expect(screen.queryByRole("link", { name: /sign in/i })).not.toBeInTheDocument();
  });
});
