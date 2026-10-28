import { describe, expect, it } from "vitest";

/**
 * Wave 0 / Phase 15: GIT-16/17 settings Danger zone rename + transfer (D-REL-07/09/10).
 * Turned green in 15-05.
 */

describe("repo settings rename/transfer danger zone (GIT-16/17)", () => {
  it("Danger zone exposes rename + transfer with type-confirm", async () => {
    const settingsSrc = await import("../components/repo/danger-zone-panel.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(settingsSrc, "Wave 0: settings must expose rename (GIT-16 / D-REL-07)").toMatch(
      /rename|Rename/,
    );
    expect(settingsSrc, "Wave 0: settings must expose transfer (GIT-17 / D-REL-09)").toMatch(
      /transfer|Transfer/,
    );
    expect(settingsSrc, "Wave 0: transfer must use type-the-repo-name confirm (D-REL-10)").toMatch(
      /confirm|Confirm|type.*name|repo\.transfer/,
    );
    expect(settingsSrc, "Transfer destination should autocomplete via OwnerLookup").toMatch(
      /OwnerLookup/,
    );
  }, 30_000);

  it("Settings gate uses can_admin for Admin danger-zone actions", async () => {
    const settingsSrc = await import("./$owner.$repo.settings.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(settingsSrc).toMatch(/can_admin/);
    const chromeSrc = await import("../components/repo/repo-chrome.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(chromeSrc, "Wave 0: chrome Settings visibility should use can_admin (15-05)").toMatch(
      /can_admin/,
    );
  }, 30_000);
});
