/**
 * /dashboard (D-19): the legacy SPA path resolves to a hard 404, not a soft
 * redirect. Under Astro/Rust serving there is no dashboard page at all — any
 * shell would mean the path silently renders. Assert it stays absent and a
 * real 404 document exists.
 */
import { existsSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));
const pagesDir = join(dir, "../pages");

describe("/dashboard Wave 0 (D-19)", () => {
  it("has no dashboard page — falls through to the generated 404", () => {
    const names = readdirSync(pagesDir);
    expect(names.filter((n) => n.startsWith("dashboard"))).toEqual([]);
    expect(existsSync(join(pagesDir, "404.astro"))).toBe(true);
  });
});
