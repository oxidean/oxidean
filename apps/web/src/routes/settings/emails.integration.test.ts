/**
 * /settings/emails redirects to Account; coverage stays on profile.integration.test.ts.
 * Under Astro the redirect lives in the page frontmatter, not a route module.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));

describe("/settings/emails redirect", () => {
  it("redirects to /settings/profile", () => {
    const src = readFileSync(join(dir, "../../pages/settings/emails.astro"), "utf8");
    expect(src).toContain('Astro.redirect("/settings/profile")');
  });
});
