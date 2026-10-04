/**
 * Admin runners view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { AdminRunnersPage } from "./runners";

describe("/admin/runners", () => {
  it("exports AdminRunnersPage", () => {
    expect(typeof AdminRunnersPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./runners.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});
