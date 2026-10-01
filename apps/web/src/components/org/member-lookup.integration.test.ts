import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemberLookup } from "@/components/org/member-lookup";
import { renderWithQueryClient } from "@/test/render-with-query";

const lookupMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    user: {
      lookup: (...args: unknown[]) => lookupMock(...args),
    },
  },
}));

describe("MemberLookup", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    lookupMock.mockReset();
    lookupMock.mockResolvedValue({
      ok: true,
      data: {
        users: [
          {
            id: "u9",
            username: "grace",
            display_name: "Grace Hopper",
            avatar_url: null,
          },
        ],
      },
    });
  });

  it("calls user.lookup with org context and never renders email", async () => {
    const onValueChange = vi.fn();
    const onPick = vi.fn();

    renderWithQueryClient(MemberLookup, {
      props: {
        id: "member-lookup",
        value: "gr",
        onValueChange,
        onPick,
        context: { kind: "org", slug: "acme" },
      },
    });

    await waitFor(
      () => {
        expect(lookupMock).toHaveBeenCalledWith({
          prefix: "gr",
          context: { kind: "org", slug: "acme" },
        });
      },
      { timeout: 3_000 },
    );

    await waitFor(() => {
      expect(screen.getByRole("option")).toBeTruthy();
      expect(screen.getByText("@grace")).toBeTruthy();
      expect(screen.getByText("Grace Hopper")).toBeTruthy();
    });

    expect(document.body.textContent).not.toMatch(/@example\.com|grace@/i);

    const optionBtn = screen.getByRole("option").querySelector("button");
    expect(optionBtn).toBeTruthy();
    fireEvent.mouseDown(optionBtn!);

    expect(onValueChange).toHaveBeenCalledWith("grace");
    expect(onPick).toHaveBeenCalledWith(
      expect.objectContaining({ username: "grace", display_name: "Grace Hopper" }),
    );
  });

  it("passes repo context for collaborator lookup", async () => {
    renderWithQueryClient(MemberLookup, {
      props: {
        id: "collab-lookup",
        value: "ad",
        onValueChange: vi.fn(),
        context: { kind: "repo", owner: "ada", name: "hello" },
      },
    });

    await waitFor(
      () => {
        expect(lookupMock).toHaveBeenCalledWith({
          prefix: "ad",
          context: { kind: "repo", owner: "ada", name: "hello" },
        });
      },
      { timeout: 3_000 },
    );
  });
});
