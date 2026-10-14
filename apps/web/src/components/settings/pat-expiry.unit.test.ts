import { describe, expect, it } from "vitest";
import {
  datePlusDays,
  defaultPresetForPolicy,
  expiryOptionsForPolicy,
  expiryPresetLabel,
  expiryToIso,
  maxCustomDate,
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
    // "none" sends the wire sentinel — an omitted expires_at means
    // "default TTL" on policy-enforcing servers, not "no expiry".
    expect(resolveExpiresAt("none", "2026-10-01")).toBe("never");
    expect(resolveExpiresAt("custom", "2026-10-01")).toBe("2026-10-01T23:59:59Z");
    expect(resolveExpiresAt("custom", "")).toBeNull();
    const in7 = resolveExpiresAt("7", "");
    expect(in7).toBe(expiryToIso(datePlusDays(7)));
    expect(resolveExpiresAt("30", "")).toBe(expiryToIso(datePlusDays(30)));
  });

  it("expiryOptionsForPolicy falls back permissively without a policy", () => {
    const opts = expiryOptionsForPolicy(null);
    expect(opts.map((o) => o.value)).toEqual(["7", "30", "60", "90", "custom", "none"]);
  });

  it("expiryOptionsForPolicy caps presets at max_ttl_days and gates never-expire", () => {
    const opts = expiryOptionsForPolicy({
      default_ttl_days: 30,
      max_ttl_days: 60,
      allow_never_expire: false,
    });
    expect(opts.map((o) => o.value)).toEqual(["7", "30", "60", "custom"]);
    expect(opts.find((o) => o.value === "30")?.label).toBe("30 days (default)");

    const withNever = expiryOptionsForPolicy({
      default_ttl_days: 14,
      max_ttl_days: 90,
      allow_never_expire: true,
    });
    expect(withNever.map((o) => o.value)).toEqual(["7", "14", "30", "60", "90", "custom", "none"]);
  });

  it("defaultPresetForPolicy and maxCustomDate track the policy", () => {
    expect(defaultPresetForPolicy(null)).toBe("30");
    expect(
      defaultPresetForPolicy({ default_ttl_days: 45, max_ttl_days: 90, allow_never_expire: false }),
    ).toBe("45");
    expect(maxCustomDate(null)).toBeUndefined();
    expect(
      maxCustomDate({ default_ttl_days: 30, max_ttl_days: 60, allow_never_expire: false }),
    ).toBe(datePlusDays(60));
  });

  it("expiryPresetLabel covers options", () => {
    const opts = expiryOptionsForPolicy(null);
    expect(expiryPresetLabel(opts, "30")).toBe("30 days");
    expect(expiryPresetLabel(opts, "none")).toBe("No expiration");
    expect(expiryPresetLabel(opts, "custom")).toBe("Custom date");
  });
});
