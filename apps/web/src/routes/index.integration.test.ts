import { cleanup, render } from "@octanejs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getQueryClient } from "@/lib/query-client";

const meMock = vi.fn();
const providerConfigMock = vi.fn();
const repoListMineMock = vi.fn();

vi.mock("@/lib/api-client", () => ({
  apiClient: {
    auth: {
      me: (...args: unknown[]) => meMock(...args),
      providerConfig: (...args: unknown[]) => providerConfigMock(...args),
    },
    repo: {
      listMine: (...args: unknown[]) => repoListMineMock(...args),
    },
  },
}));

import { AnonLanding, HomePage, selectHomeTree } from "./index";

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
 * stamp on <html> can pick via CSS. Resolution drops the skeleton candidate
 * only — the `data-anon-landing` wrapper is retained so the landing subtree
 * keeps its DOM position and the hero never remounts (issue #143). Reveal
 * re-observation is covered in index.browser.test.tsx.
 */
describe("index pending dual-render", () => {
  beforeEach(() => {
    meMock.mockReset();
    providerConfigMock.mockReset();
    providerConfigMock.mockResolvedValue({
      ok: true,
      data: { mode: "local", allow_signup: true },
    });
    repoListMineMock.mockReset();
    repoListMineMock.mockResolvedValue({ ok: true, data: { repos: [] } });
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

  it("anonymous resolution drops the skeleton wrapper, keeps the landing wrapper", async () => {
    const me = deferred<unknown>();
    meMock.mockReturnValue(me.promise);

    render(HomePage, {});
    const landingBefore = document.querySelector("[data-anon-landing]")!;
    expect(landingBefore).toBeTruthy();
    me.resolve(ANON);

    await vi.waitFor(() => {
      expect(document.querySelector("#explore")).toBeTruthy();
      expect(document.querySelector("[data-anon-landing]")).not.toBeNull();
      expect(document.querySelector("[data-home-skeleton]")).toBeNull();
    });
    // The landing wrapper is the very same node — no remount (issue #143).
    expect(document.querySelector("[data-anon-landing]")).toBe(landingBefore);
  });
});

/**
 * Issue #120 — signed-in home must go skeleton → populated. While
 * `repo.listMine` is still in flight `reposQ.data` is `undefined`; rendering
 * `SignedInHome` with `repos=[]` flashed the "No repositories yet." empty
 * state before the populated list arrived.
 */
describe("index signed-in repo loading (no empty-state flicker)", () => {
  const user = {
    id: "u1",
    username: "ada",
    email: "ada@example.com",
    display_name: "Ada",
    bio: "",
    avatar_url: null,
    role: "user",
    profile_incomplete: false,
    email_verified: true,
    must_change_credentials: false,
    default_branch: "main",
  };

  beforeEach(() => {
    meMock.mockReset();
    providerConfigMock.mockReset();
    providerConfigMock.mockResolvedValue({
      ok: true,
      data: { mode: "local", allow_signup: true },
    });
    repoListMineMock.mockReset();
    getQueryClient().clear();
  });

  afterEach(() => {
    getQueryClient().clear();
    cleanup();
  });

  it("keeps the skeleton while repo.listMine is pending, never flashes empty state", async () => {
    const me = deferred<unknown>();
    const repos = deferred<unknown>();
    meMock.mockReturnValue(me.promise);
    repoListMineMock.mockReturnValue(repos.promise);

    render(HomePage, {});

    me.resolve({ ok: true, data: user });

    // Session resolved signed-in but the repo list is still in flight: the
    // skeleton must stay up — the empty state must never render as a
    // transient before the populated tree.
    await vi.waitFor(() => {
      expect(document.querySelector('[data-testid="signed-in-home"]')).toBeNull();
      expect(document.querySelector("[data-home-skeleton]")).toBeNull();
      expect(document.querySelector('[aria-busy="true"][aria-label="Loading"]')).toBeTruthy();
    });
    expect(document.body.textContent).not.toContain("No repositories yet.");
    expect(document.body.textContent).not.toContain("Create your first repository");

    repos.resolve({
      ok: true,
      data: {
        repos: [
          {
            id: "r1",
            owner_id: "u1",
            owner_type: "user",
            owner_username: "ada",
            name: "hello",
            description: "",
            visibility: "public",
            default_branch: "main",
            updated_at: "2026-09-12T12:00:00Z",
          },
        ],
      },
    });

    await vi.waitFor(() => {
      expect(document.querySelector('[data-testid="signed-in-home"]')).toBeTruthy();
    });
    expect(document.body.textContent).toContain("ada/hello");
    expect(document.body.textContent).not.toContain("No repositories yet.");
  });
});

/**
 * Issue #165 — a `;` typed into JSX children position renders as a literal
 * text node. The shipped instance was `</AppLink>;` between the hero CTAs
 * ("Get started ; Explore"). Rendering `AnonLanding` directly covers the
 * marketing tree without the auth.me dance; the structural gate for every
 * template is scripts/check-template-semicolons.ts (runs in `make web-lint`).
 */
describe("index landing markup hygiene (issue #165)", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders no stray `;` text nodes anywhere", () => {
    render(AnonLanding, {});
    const strays: string[] = [];
    const walk = (node: Node) => {
      for (const child of Array.from(node.childNodes)) {
        if (child.nodeType === 3 && /^\s*;+\s*$/.test(child.nodeValue ?? "")) {
          strays.push(child.nodeValue ?? "");
        }
        walk(child);
      }
    };
    walk(document.body);
    expect(strays).toEqual([]);
  });

  it("hero CTAs are exactly the two anchors — no interstitial text", () => {
    render(AnonLanding, {});
    const ctas = document.querySelectorAll("[data-anon-signup-cta]");
    expect(ctas.length).toBe(2);
    for (const cta of Array.from(ctas)) {
      const parent = cta.parentElement!;
      // Elements and Octane's comment anchors are fine; a non-whitespace text
      // node here is a leaked statement terminator (or stray punctuation).
      const strayText = Array.from(parent.childNodes).filter(
        (n) => n.nodeType === 3 && (n.nodeValue ?? "").trim() !== "",
      );
      expect(strayText.map((n) => n.nodeValue)).toEqual([]);
      expect(parent.children).toHaveLength(2);
    }
  });
});
