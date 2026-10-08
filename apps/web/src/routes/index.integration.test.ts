import { describe, expect, it } from "vitest";
import { HomePage, selectHomeTree } from "./index";

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
