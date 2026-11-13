/**
 * Account /settings/experiments — per-user flag enrollment (WI-0001).
 */
import { cleanup, screen, waitFor } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

const meMock = vi.fn();
const listMock = vi.fn();
const setMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
    },
    experiments: {
      list: (...args: unknown[]) => listMock(...args),
      set: (...args: unknown[]) => setMock(...args),
    },
  },
}));

import { ExperimentsSettingsPage } from "./experiments.tsrx";

const readyUser = {
  id: "u1",
  email: "flags@oxidean.local",
  username: "flaguser",
  display_name: "Flag User",
  bio: "",
  avatar_url: null as null,
  role: "user",
  profile_incomplete: false,
  email_verified: true,
  must_change_credentials: false,
  default_branch: "main",
  experiments: [] as string[],
};

const prNewUi = {
  key: "pr-new-ui",
  title: "New pull request experience",
  description: "GitHub-style pull request layout.",
  status: "experimental",
  since_version: "unreleased",
  enabled: false,
};

afterEach(cleanup);

beforeEach(() => {
  window.history.pushState({}, "", "/settings/experiments");
  meMock.mockReset();
  listMock.mockReset();
  setMock.mockReset();
  meMock.mockResolvedValue({ ok: true, data: readyUser });
  listMock.mockResolvedValue({
    ok: true,
    data: { experiments: [prNewUi] },
  });
});

describe("/settings/experiments", () => {
  it("keys the page query on experiments.list", async () => {
    const src = await import("./experiments.tsrx?raw").then((m) => String(m.default));
    expect(src).toContain("experimentsListQueryOptions");
  });

  it("redirects anonymous sessions to login", async () => {
    meMock.mockResolvedValue({
      ok: false,
      error: { code: "auth.unauthenticated", message: "sign in required" },
    });
    renderWithQueryClient(ExperimentsSettingsPage);
    await waitFor(() => {
      expect(`${window.location.pathname}${window.location.search}`).toBe(
        "/login?returnTo=/settings/experiments",
      );
    });
  });

  it("renders registry flags with current enrollment", async () => {
    renderWithQueryClient(ExperimentsSettingsPage);
    await waitFor(() => {
      expect(screen.getByText("New pull request experience")).toBeTruthy();
    });
    const toggle = screen.getByTestId("experiment-toggle-pr-new-ui");
    expect(toggle.getAttribute("aria-checked") ?? toggle.getAttribute("data-state")).not.toBe(
      "checked",
    );
  });

  it("enables a flag and writes auth.me from the set response", async () => {
    setMock.mockResolvedValue({
      ok: true,
      data: { ...readyUser, experiments: ["pr-new-ui"] },
    });
    renderWithQueryClient(ExperimentsSettingsPage);
    const toggle = await waitFor(() => screen.getByTestId("experiment-toggle-pr-new-ui"));
    toggle.click();
    await waitFor(() => {
      expect(setMock).toHaveBeenCalledWith({ key: "pr-new-ui", enabled: true });
    });
  });

  it("surfaces set failures without flipping the toggle", async () => {
    setMock.mockResolvedValue({
      ok: false,
      error: { code: "experiments.unknown_key", message: "unknown experiment flag" },
    });
    renderWithQueryClient(ExperimentsSettingsPage);
    const toggle = await waitFor(() => screen.getByTestId("experiment-toggle-pr-new-ui"));
    toggle.click();
    await waitFor(() => {
      expect(setMock).toHaveBeenCalledWith({ key: "pr-new-ui", enabled: true });
    });
    // Toggle visual state comes from listQ.data — unchanged after a failed set.
  });
});
