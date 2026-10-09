import { describe, expect, it } from "vitest";
import { matchPath, repoParamsFromPathname } from "./route-params";

describe("matchPath", () => {
  it("binds named params and percent-decodes them", () => {
    expect(matchPath("/$owner/$repo", "/ada/hello%20world")).toEqual({
      owner: "ada",
      repo: "hello world",
    });
  });

  it("binds the rest of the path under a bare $ splat", () => {
    expect(matchPath("/$owner/$repo/blob/$", "/ada/app/blob/main/src/lib/x.ts")).toEqual({
      owner: "ada",
      repo: "app",
      _splat: "main/src/lib/x.ts",
    });
  });

  it("returns null on literal mismatch and arity mismatch", () => {
    expect(matchPath("/settings/general", "/settings/profile")).toBeNull();
    expect(matchPath("/$owner/$repo", "/ada")).toBeNull();
    expect(matchPath("/$owner", "/ada/app")).toBeNull();
  });

  it("does not throw on malformed percent sequences", () => {
    // `decodeURIComponent("100%")` throws URIError — the segment must degrade
    // to its raw form instead of crashing the island.
    expect(matchPath("/$owner/$repo", "/ada/100%")).toEqual({
      owner: "ada",
      repo: "100%",
    });
    expect(matchPath("/$owner/$repo/releases/$", "/ada/app/releases/v1%")).toEqual({
      owner: "ada",
      repo: "app",
      _splat: "v1%",
    });
  });
});

describe("repoParamsFromPathname", () => {
  it("extracts owner/repo from repo paths and skips reserved segments", () => {
    expect(repoParamsFromPathname("/ada/app/issues")).toEqual({ owner: "ada", repo: "app" });
    expect(repoParamsFromPathname("/settings/general")).toBeNull();
    expect(repoParamsFromPathname("/dashboard")).toBeNull();
    expect(repoParamsFromPathname("/ada/settings")).toBeNull();
  });
});
