/**
 * Phase 17 — SiteHeader notifications bell (D-08 / 17-UI-SPEC / NOTF-02).
 */
import { createElement } from "octane";
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

vi.mock("@octanejs/tanstack-router", () => ({
  Link: (props: {
    to?: string;
    children?: unknown;
    className?: string;
    onClick?: () => void;
    "aria-label"?: string;
  }) =>
    createElement(
      "a",
      {
        href: props.to ?? "#",
        className: props.className,
        onClick: props.onClick,
        "aria-label": props["aria-label"],
      },
      props.children as never,
    ),
  useNavigate: () => () => undefined,
  useParams: () => ({}),
  // No RouterProvider — useAppNavigate must see "no router" and fall back to
  // location.assign (spied on in tests below).
  useRouter: () => undefined,
}));

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: vi.fn(),
      bootstrapStatus: vi.fn(),
      providerConfig: vi.fn(),
      logout: vi.fn(async () => ({ ok: true, data: { ok: true } })),
    },
    org: {
      listMine: vi.fn(async () => ({ ok: true, data: { orgs: [] } })),
    },
    notification: {
      unreadCount: vi.fn(async () => ({ ok: true, data: { count: 3 } })),
      list: vi.fn(),
      markRead: vi.fn(),
      markAllRead: vi.fn(),
    },
  },
}));

import { apiClient } from "@/lib/api-client";
import { SiteHeader } from "./chrome";

afterEach(cleanup);

beforeEach(() => {
  vi.mocked(apiClient.auth.bootstrapStatus).mockResolvedValue({
    ok: true,
    data: { needs_setup: false },
  } as never);
  vi.mocked(apiClient.auth.providerConfig).mockResolvedValue({
    ok: true,
    data: { mode: "local", allow_signup: false },
  } as never);
});

describe("SiteHeader notifications bell (D-08 / NOTF-02)", () => {
  it("signed-in chrome exposes notifications control with unread count", async () => {
    vi.mocked(apiClient.auth.me).mockResolvedValue({
      ok: true,
      data: {
        id: "u1",
        email: "a@ex.com",
        username: "ada",
        display_name: "Ada",
        bio: "",
        role: "user",
        profile_incomplete: false,
        email_verified: true,
        must_change_credentials: false,
        default_branch: "main",
      },
    } as never);

    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /notifications \(3 unread\)/i })).toBeInTheDocument();
    });
    expect(screen.getByRole("link", { name: /notifications/i })).toHaveAttribute(
      "href",
      "/notifications",
    );
  });

  it("anonymous chrome has no notifications bell", async () => {
    vi.mocked(apiClient.auth.me).mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "n" },
    } as never);

    renderWithQueryClient(SiteHeader);

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /sign in/i })).toBeInTheDocument();
    });
    expect(screen.queryByRole("link", { name: /notifications/i })).not.toBeInTheDocument();
  });
});
