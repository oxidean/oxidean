import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));

describe("explore route (SOC-03)", () => {
  it("defines ExplorePage export in explore.tsrx", () => {
    const src = readFileSync(join(dir, "explore.tsrx"), "utf8");
    expect(src).toMatch(/export function ExplorePage/);
    expect(src).toMatch(/"repo", "explore"/);
  });
});
