/**
 * Repo blame view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { RepoBlamePage } from "./$owner.$repo.blame.$";

describe("/$owner/$repo/blame/$", () => {
  it("exports RepoBlamePage", () => {
    expect(typeof RepoBlamePage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.blame.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});

/**
 * Issue #110 — blame mounts rows incrementally instead of the full file.
 */
import { cleanup, fireEvent, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";
import { BLAME_LINES_PAGE_SIZE } from "@/lib/diff-budget";

const repoGetMock = vi.fn();
const refsMock = vi.fn();
const blameMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: vi.fn(async () => ({
        ok: false,
        error: { code: "auth.unauthenticated", message: "anonymous" },
      })),
    },
    repo: {
      get: (...args: unknown[]) => repoGetMock(...args),
      refs: (...args: unknown[]) => refsMock(...args),
      blame: (...args: unknown[]) => blameMock(...args),
    },
  },
}));

const repo = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user" as const,
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public" as const,
  default_branch: "main",
  updated_at: "2026-10-02T00:00:00Z",
  can_admin: false,
  can_write: false,
  issues_enabled: true,
  pulls_enabled: true,
};

const SHA = "0123456789abcdef0123456789abcdef01234567";

function blameLines(count: number) {
  return Array.from({ length: count }, (_, i) => ({
    sha: SHA,
    author_name: "Ada Lovelace",
    author_email: "ada@example.com",
    authored_at: "2026-10-02T12:00:00Z",
    line_number: i + 1,
    content: `line ${i + 1}`,
    author_username: "ada",
  }));
}

beforeEach(() => {
  window.history.pushState({}, "", "/ada/hello/blame/main/big.ts");
  repoGetMock.mockReset();
  refsMock.mockReset();
  blameMock.mockReset();
  repoGetMock.mockResolvedValue({ ok: true, data: repo });
  refsMock.mockResolvedValue({
    ok: true,
    data: { refs: [{ name: "refs/heads/main", oid: SHA }] },
  });
});

afterEach(cleanup);

describe("incremental blame rows (issue #110)", () => {
  it("mounts the first page of blame rows, then grows on Load more", async () => {
    blameMock.mockResolvedValue({
      ok: true,
      data: {
        path: "big.ts",
        ref: "main",
        lines: blameLines(BLAME_LINES_PAGE_SIZE + 25),
        truncated: false,
      },
    });

    renderWithQueryClient(RepoBlamePage);

    await waitFor(() => {
      expect(document.querySelectorAll("tbody tr").length).toBe(BLAME_LINES_PAGE_SIZE);
    });
    expect(blameMock).toHaveBeenCalledWith({
      owner: "ada",
      name: "hello",
      ref: "main",
      path: "big.ts",
    });

    fireEvent.click(screen.getByRole("button", { name: /Load more lines/ }));

    await waitFor(() => {
      expect(document.querySelectorAll("tbody tr").length).toBe(BLAME_LINES_PAGE_SIZE + 25);
    });
  });
});
