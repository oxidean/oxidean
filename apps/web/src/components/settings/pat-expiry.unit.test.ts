import { describe, expect, it } from "vitest";
import {
  datePlusDays,
  expiryPresetLabel,
  expiryToIso,
  resolveExpiresAt,
  tomorrowDateValue,
} from "@/components/settings/pat-expiry";

describe("pat-expiry helpers", () => {
  it("tomorrowDateValue is YYYY-MM-DD", () => {
    expect(tomorrowDateValue()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it("expiryToIso appends end-of-day Z", () => {
    expect(expiryToIso("2026-10-01")).toBe("2026-10-01T23:59:59Z");
  });

  it("resolveExpiresAt maps presets", () => {
    expect(resolveExpiresAt("none", "2026-10-01")).toBeNull();
    expect(resolveExpiresAt("custom", "2026-10-01")).toBe("2026-10-01T23:59:59Z");
    expect(resolveExpiresAt("custom", "")).toBeNull();
    const in7 = resolveExpiresAt("7", "");
    expect(in7).toBe(expiryToIso(datePlusDays(7)));
    expect(resolveExpiresAt("30", "")).toBe(expiryToIso(datePlusDays(30)));
  });

  it("expiryPresetLabel covers catalog", () => {
    expect(expiryPresetLabel("30")).toBe("30 days");
    expect(expiryPresetLabel("none")).toBe("No expiration");
  });
});
