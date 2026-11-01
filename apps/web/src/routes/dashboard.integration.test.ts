/**
 * /dashboard (D-19): the legacy SPA path resolves to a hard 404, not a soft
 * redirect. Under Astro/Rust serving there is no dashboard page at all — any
 * shell would mean the path silently renders. Assert it stays absent; misses
 * answer the Rust tier's embedded branded status page (crates/oxidean-status-pages),
 * so no Astro 404 document exists to point at.
 */
import { readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));
const pagesDir = join(dir, "../pages");

describe("/dashboard Wave 0 (D-19)", () => {
  it("has no dashboard page — falls through to the Rust-served 404", () => {
    const names = readdirSync(pagesDir);
    expect(names.filter((n) => n.startsWith("dashboard"))).toEqual([]);
  });
});
