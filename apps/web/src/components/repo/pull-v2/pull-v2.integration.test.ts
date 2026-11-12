/**
 * pull-v2 scaffold (WI-0001, stage 3): splat → tab mapping, sticky chrome,
 * URL-backed tab navigation, and per-tab delegation.
 */
import { cleanup, screen } from "@octanejs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithQueryClient } from "@/test/render-with-query";

vi.mock("@/components/repo/pull-conversation", () => ({
  PullConversation: () => null,
}));
vi.mock("@/components/repo/pull-reviews", () => ({
  PullReviews: () => null,
}));
vi.mock("@/components/repo/pull-commits", () => ({
  PullCommits: () => null,
}));
vi.mock("@/components/repo/pull-checks", () => ({
  PullChecks: () => null,
}));
vi.mock("@/components/repo/pull-files", () => ({
  PullFiles: () => null,
}));

import { PullDetailV2, pullV2TabFromSplat } from "./pull-v2.tsrx";

const PULL = {
  number: 42,
  title: "Wire the thing",
  state: "open",
  draft: false,
  author_id: "u1",
  author_username: "ada",
  head_owner: "ada",
  head_name: "hello",
  head_ref: "feature/x",
  head_sha: "abc123",
  base_ref: "main",
  body: "",
};

const REPO = { name: "hello", can_write: true, archived: false };

function renderV2(tab: "overview" | "commits" | "checks" | "changes") {
  return renderWithQueryClient(PullDetailV2, {
    props: {
      owner: "ada",
      repoName: "hello",
      number: 42,
      repo: REPO as never,
      pull: PULL as never,
      canWrite: true,
      canInteract: true,
      isAuthor: true,
      currentUserId: "u1",
      tab,
    },
  });
}

afterEach(cleanup);

describe("pullV2TabFromSplat", () => {
  it.each([
    ["", "overview"],
    ["commits", "commits"],
    ["checks", "checks"],
    ["changes", "changes"],
    ["changes/abc123", "changes"],
    ["bogus", "overview"],
  ])("maps %s → %s", (splat, expected) => {
    expect(pullV2TabFromSplat(splat)).toBe(expected);
  });
});

describe("PullDetailV2 chrome", () => {
  it("renders the number, title, state, and URL-backed tabs", () => {
    renderV2("overview");
    expect(screen.getByText("#42")).toBeTruthy();
    expect(screen.getByText("Wire the thing")).toBeTruthy();
    expect(screen.getByText("Open")).toBeTruthy();

    const overview = screen.getByTestId("pull-v2-tab-overview");
    expect(overview.getAttribute("href")).toBe("/ada/hello/pull/42");
    expect(overview.getAttribute("aria-current")).toBe("page");

    expect(screen.getByTestId("pull-v2-tab-commits").getAttribute("href")).toBe(
      "/ada/hello/pull/42/commits",
    );
    expect(screen.getByTestId("pull-v2-tab-checks").getAttribute("href")).toBe(
      "/ada/hello/pull/42/checks",
    );
    expect(screen.getByTestId("pull-v2-tab-changes").getAttribute("href")).toBe(
      "/ada/hello/pull/42/changes",
    );
  });

  it("marks the active tab with aria-current only", () => {
    renderV2("changes");
    expect(screen.getByTestId("pull-v2-tab-changes").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("pull-v2-tab-overview").getAttribute("aria-current")).toBeNull();
  });

  it("shows draft + merged states", () => {
    const draft = { ...PULL, draft: true };
    renderWithQueryClient(PullDetailV2, {
      props: {
        owner: "ada",
        repoName: "hello",
        number: 42,
        repo: REPO as never,
        pull: draft as never,
        canWrite: true,
        canInteract: true,
        isAuthor: true,
        currentUserId: "u1",
        tab: "overview",
      },
    });
    expect(screen.getByText("Draft")).toBeTruthy();
    cleanup();
    const merged = { ...PULL, state: "merged" };
    renderWithQueryClient(PullDetailV2, {
      props: {
        owner: "ada",
        repoName: "hello",
        number: 42,
        repo: REPO as never,
        pull: merged as never,
        canWrite: true,
        canInteract: true,
        isAuthor: true,
        currentUserId: "u1",
        tab: "overview",
      },
    });
    expect(screen.getByText("Merged")).toBeTruthy();
  });
});
