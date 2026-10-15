import { cleanup, fireEvent, render, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getQueryClient } from "@/lib/query-client";
import { authMeQueryKey } from "@/lib/session-queries";

/**
 * ORG-01 / D-ORG-01 / D-ORG-06: /orgs/new create UI (10-13 tracer).
 */

const createMock = vi.fn();
const meMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    org: {
      create: (...args: unknown[]) => createMock(...args),
    },
  },
}));

const user = {
  id: "u1",
  email: "ada@example.com",
  username: "ada",
  display_name: "Ada",
  bio: "",
  avatar_url: null as null,
  role: "user",
  profile_incomplete: false,
  email_verified: false,
};

beforeEach(() => {
  window.history.pushState({}, "", "/orgs/new");
  createMock.mockReset();
  user.email_verified = false;
  meMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: user });
});

import { OrgsNewPage } from "./orgs.new";

afterEach(cleanup);

describe("/orgs/new (ORG-01 / D-ORG-01 / D-ORG-06)", () => {
  // Issue #143 regression — same wall/form mounted gate as /new: the first
  // commit must match the prerendered wall even with a warm auth.me cache.
  it("warm auth.me cache commits the SSR wall first, then swaps to the form", async () => {
    const client = getQueryClient();
    client.clear();
    client.setQueryData(authMeQueryKey, { ...user, email_verified: true });

    const wallText = "Verify your email";
    const commits: string[] = [];
    const observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of record.addedNodes) {
          if (node.textContent?.includes(wallText)) commits.push("wall-mounted");
        }
        for (const node of record.removedNodes) {
          if (node.textContent?.includes(wallText)) commits.push("wall-removed");
        }
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    try {
      render(OrgsNewPage);

      await waitFor(() => {
        expect(
          screen.getByRole("heading", { name: "Create a new organization" }),
        ).toBeInTheDocument();
      });
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(commits[0]).toBe("wall-mounted");
      expect(commits).toContain("wall-removed");
    } finally {
      observer.disconnect();
      client.clear();
    }
  }, 20000);

  it("verified: Slug + optional Display name + Create organization CTA", async () => {
    user.email_verified = true;
    render(OrgsNewPage);

    await waitFor(() => {
      expect(
        screen.getByRole("heading", { name: "Create a new organization" }),
      ).toBeInTheDocument();
    });
    expect(screen.getByLabelText("Slug")).toBeInTheDocument();
    expect(screen.getByLabelText("Display name (optional)")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create organization" })).toBeInTheDocument();

    createMock.mockResolvedValueOnce({
      ok: true,
      data: {
        id: "o1",
        slug: "acme",
        display_name: "Acme",
        member_base_permission: "none",
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      },
    });
    const pushState = vi.spyOn(window.history, "pushState");

    fireEvent.input(screen.getByLabelText("Slug"), {
      target: { value: "acme" },
    });
    fireEvent.input(screen.getByLabelText("Display name (optional)"), {
      target: { value: "Acme" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create organization" }));

    await waitFor(() => {
      expect(createMock).toHaveBeenCalledWith({
        slug: "acme",
        display_name: "Acme",
      });
    });
    await waitFor(() => {
      expect(pushState).toHaveBeenCalledWith({}, "", "/acme");
    });
    pushState.mockRestore();
  });

  it("unverified: Verify your email wall — not the create form", async () => {
    render(OrgsNewPage);

    await waitFor(() => {
      expect(screen.getByText("Verify your email")).toBeInTheDocument();
    });
    expect(
      screen.getByText("Verify your email before creating an organization."),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("Slug")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Create organization" })).not.toBeInTheDocument();
  });

  it("reserved slug shows That username is reserved. Choose a different username.", async () => {
    user.email_verified = true;
    render(OrgsNewPage);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Create organization" })).toBeInTheDocument();
    });

    createMock.mockResolvedValueOnce({
      ok: false,
      error: { code: "auth.reserved_username", message: "username is reserved" },
    });

    fireEvent.input(screen.getByLabelText("Slug"), {
      target: { value: "orgs" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create organization" }));

    await waitFor(() => {
      expect(
        screen.getByText("That username is reserved. Choose a different username."),
      ).toBeInTheDocument();
    });
  });

  it("taken slug shows slug already used by a user or org", async () => {
    user.email_verified = true;
    render(OrgsNewPage);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Create organization" })).toBeInTheDocument();
    });

    createMock.mockResolvedValueOnce({
      ok: false,
      error: {
        code: "org.slug_taken",
        message: "That slug is already used by a user or organization. Choose a different slug.",
      },
    });

    fireEvent.input(screen.getByLabelText("Slug"), {
      target: { value: "taken-slug" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create organization" }));

    await waitFor(() => {
      expect(
        screen.getByText(
          "That slug is already used by a user or organization. Choose a different slug.",
        ),
      ).toBeInTheDocument();
    });
  });
});
