import { describe, expect, it } from "vitest";
import { repoChromeActiveFromPath } from "./repo-chrome-active";

describe("repoChromeActiveFromPath", () => {
  it("maps /owner/repo/packages → packages", () => {
    expect(repoChromeActiveFromPath("/ada/hello/packages")).toBe("packages");
  });

  it("maps /owner/repo/actions → actions", () => {
    expect(repoChromeActiveFromPath("/ada/hello/actions")).toBe("actions");
    expect(repoChromeActiveFromPath("/ada/hello/actions/abc")).toBe("actions");
  });

  it("maps /owner/repo/issues (and nested) → issues", () => {
    expect(repoChromeActiveFromPath("/ada/hello/issues")).toBe("issues");
    expect(repoChromeActiveFromPath("/ada/hello/issues/12")).toBe("issues");
  });

  it("maps /owner/repo → code", () => {
    expect(repoChromeActiveFromPath("/ada/hello")).toBe("code");
    expect(repoChromeActiveFromPath("/ada/hello/")).toBe("code");
  });

  it("maps /owner/repo/insights → insights", () => {
    expect(repoChromeActiveFromPath("/ada/hello/insights")).toBe("insights");
  });

  it("maps known chrome segments; unknown → code", () => {
    expect(repoChromeActiveFromPath("/ada/hello/releases")).toBe("releases");
    expect(repoChromeActiveFromPath("/ada/hello/settings")).toBe("settings");
    expect(repoChromeActiveFromPath("/ada/hello/commits/main")).toBe("commits");
    expect(repoChromeActiveFromPath("/ada/hello/branches")).toBe("branches");
    expect(repoChromeActiveFromPath("/ada/hello/tags")).toBe("tags");
    expect(repoChromeActiveFromPath("/ada/hello/tree/main")).toBe("code");
  });
});
