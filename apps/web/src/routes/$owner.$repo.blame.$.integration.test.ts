/**
 * Repo blame view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { RepoBlamePage } from "./$owner.$repo.blame.$";

describe("/$owner/$repo/blame/$", () => {
  it("exports RepoBlamePage", () => {
    expect(typeof RepoBlamePage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./$owner.$repo.blame.$.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});
