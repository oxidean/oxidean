import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const getMock = vi.fn();
const listMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    repo: {
      get: (...args: unknown[]) => getMock(...args),
    },
    release: {
      list: (...args: unknown[]) => listMock(...args),
    },
  },
}));

const readableRepo = {
  id: "r1",
  owner_id: "u1",
  owner_type: "user" as const,
  owner_username: "ada",
  name: "hello",
  description: "",
  visibility: "public" as const,
  default_branch: "main",
  updated_at: "2026-09-14T00:00:00Z",
  can_admin: true,
  can_write: true,
};

import { RepoReleasesPage } from "./$owner.$repo.releases.index";

afterEach(cleanup);

/**
 * Wave 0 / Phase 15: GIT-14/15 Releases tab + routes (D-REL-13).
 * Turned green in 15-06 (notes) / 15-02 (assets).
 */

describe("repo Releases tab (GIT-14/15 / D-REL-13)", () => {
  it("Releases tab present in repo chrome", async () => {
    const chromeSrc = await import("../components/repo/repo-chrome.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(chromeSrc, "Wave 0: repo-chrome must include Releases tab (D-REL-13)").toMatch(
      /Releases/,
    );
    expect(chromeSrc).toMatch(/releases/);
    expect(chromeSrc).toMatch(/active === "releases"/);
  }, 30_000);

  it("releases leaves do not remount RepoChrome (D-QH-01)", async () => {
    const sources = await Promise.all([
      import("./$owner.$repo.releases.index.tsrx?raw"),
      import("./$owner.$repo.releases.new.tsrx?raw"),
      import("./$owner.$repo.releases.$tag.tsrx?raw"),
    ]);
    const names = ["releases.index", "releases.new", "releases.$tag"];
    for (let i = 0; i < sources.length; i++) {
      const src = String((sources[i] as { default: string }).default);
      expect(src, `${names[i]} must not remount RepoChrome`).not.toMatch(/RepoChrome/);
    }
  }, 30_000);

  it("routes under /{owner}/{repo}/releases discoverable", async () => {
    const list = await import("./$owner.$repo.releases.index").catch(() => null);
    const create = await import("./$owner.$repo.releases.new").catch(() => null);
    const detail = await import("./$owner.$repo.releases.$tag").catch(() => null);
    expect(
      list?.RepoReleasesPage ?? list?.default ?? list,
      "Wave 0: $owner.$repo.releases route must export (15-06)",
    ).toBeTruthy();
    expect(
      create?.RepoReleaseNewPage ?? create?.default ?? create,
      "Wave 0: $owner.$repo.releases.new route must export (15-06)",
    ).toBeTruthy();
    expect(
      detail?.RepoReleaseDetailPage ?? detail?.default ?? detail,
      "Wave 0: $owner.$repo.releases.$tag route must export (15-06)",
    ).toBeTruthy();
  }, 30_000);
});

describe("/$owner/$repo/releases/ render mount (G-11.1-15)", () => {
  beforeEach(() => {
    window.history.pushState({}, "", "/ada/hello/releases");
    getMock.mockReset();
    listMock.mockReset();
    getMock.mockResolvedValue({ ok: true, data: readableRepo });
    listMock.mockResolvedValue({ ok: true, data: { releases: [] } });
  });

  it("renders empty releases list with draft CTA for can_write repo", async () => {
    renderWithQueryClient(RepoReleasesPage);

    await waitFor(
      () => {
        expect(screen.getByRole("heading", { name: "Releases" })).toBeTruthy();
        expect(screen.getByText(/No releases yet/i)).toBeTruthy();
        expect(screen.getByRole("button", { name: "Draft a new release" })).toBeTruthy();
      },
      { timeout: 10_000 },
    );
  });
});
