/**
 * Fine-grained PAT creation view — route coverage (D-QH-03).
 */
import { describe, expect, it } from "vitest";
import { TokensNewFineGrainedPage } from "./tokens.new.fine-grained";

describe("/settings/tokens/new/fine-grained", () => {
  it("exports TokensNewFineGrainedPage", () => {
    expect(typeof TokensNewFineGrainedPage).toBe("function");
  });

  it("uses AppLink for internal navigation (client-side routing)", async () => {
    const src = await import("./tokens.new.fine-grained.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/AppLink/);
    expect(src).not.toMatch(/<a\s+href=\{?["'`]\//);
  });
});
