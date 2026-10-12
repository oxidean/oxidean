import { cleanup, render } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getQueryClient } from "@/lib/query-client";

const meMock = vi.fn();
const providerConfigMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      providerConfig: (...args: unknown[]) => providerConfigMock(...args),
    },
    repo: {
      listMine: vi.fn(async () => ({ ok: true, data: [] })),
    },
  },
}));

import { HomePage, selectHomeTree } from "./index";

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const ANON = { ok: false, error: { code: "auth.unauthenticated", message: "" } };

/**
 * Home SSR/tree gate priority (D-18/D-20).
 * needs_setup redirects are owned by AppAccessGate; the index picks
 * SignedInHome vs marketing via the session query after that gate clears.
 */
describe("index/home SSR tree gate (D-18/D-20)", () => {
  it("gates trees via the session query (needs_setup | SignedInHome | marketing)", async () => {
    const src = await import("./index.tsrx?raw").then((m) => String(m.default));
    expect(
      /authSessionQueryOptions|useQuery/.test(src),
      "index must select trees via the session query (signed-in | marketing)",
    ).toBe(true);
    expect(HomePage, "index page component must exist for tree selection").toBeTruthy();
  });

  it("needs_setup priority selects /setup over marketing and SignedInHome", () => {
    expect(selectHomeTree({ needs_setup: true, hasSession: false })).toBe("setup");
    expect(selectHomeTree({ needs_setup: false, hasSession: true })).toBe("signed-in");
    expect(selectHomeTree({ needs_setup: false, hasSession: false })).toBe("marketing");
  });

  it("SignedInHome module exists for the session tree (D-20)", async () => {
    const signedIn = await import("@/components/signed-in-home");
    expect(signedIn).toHaveProperty("SignedInHome");
    expect(HomePage, "index route component must exist for tree selection").toBeTruthy();
  });

  it("session read is the boundary — no client redirectIfNeedsSetup reintroduced", async () => {
    const src = await import("./index.tsrx?raw").then((m) => String(m.default));
    expect(
      /redirectIfNeedsSetup/.test(src),
      "index must not reintroduce client redirectIfNeedsSetup as the boundary",
    ).toBe(false);
  });
});

/**
 * Pending dual-render: while `auth.me` is unsettled the page emits both the
 * anonymous landing and the signed-in skeleton so the `data-oxidean-session`
 * stamp on <html> can pick via CSS. Resolution must swap to a single tree and
 * drop the wrappers — the scroll-reveal observer watches live nodes only if
 * the effect re-runs on the swap (covered by index.browser.test.tsx).
 */
describe("index pending dual-render", () => {
  beforeEach(() => {
    meMock.mockReset();
    providerConfigMock.mockReset();
    providerConfigMock.mockResolvedValue({
      ok: true,
      data: { mode: "local", allow_signup: true },
    });
    getQueryClient().clear();
  });

  afterEach(() => {
    getQueryClient().clear();
    cleanup();
  });

  it("pending renders [data-anon-landing] + [data-home-skeleton]", async () => {
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    render(HomePage, {});

    expect(document.querySelector("[data-anon-landing]")).toBeTruthy();
    expect(document.querySelector("[data-home-skeleton]")).toBeTruthy();
    // Landing content is present inside its candidate wrapper.
    expect(document.querySelector("[data-anon-landing] .oct-reveal")).toBeTruthy();
  });

  it("anonymous resolution swaps to the bare landing tree (wrappers dropped)", async () => {
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    render(HomePage, {});
    me.resolve(ANON);

    await vi.waitFor(() => {
      expect(document.querySelector("#explore")).toBeTruthy();
      expect(document.querySelector("[data-anon-landing]")).toBeNull();
      expect(document.querySelector("[data-home-skeleton]")).toBeNull();
    });
  });
});
