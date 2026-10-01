import { describe, expect, it } from "vitest";

/**
 * ORG-03 / D-ORG-02c / D-ORG-04: repo settings Collaborators panel.
 */

describe("repo settings Collaborators (ORG-03 / D-ORG-02c / D-ORG-04)", () => {
  it("settings gate uses can_admin — not me.id === owner_id", async () => {
    const panel = await import("../components/repo/collaborators-panel");
    expect(
      panel.CollaboratorsPanel ?? panel.default,
      "CollaboratorsPanel must export for can_admin-gated settings (D-ORG-04)",
    ).toBeTruthy();

    const settings = await import("./$owner.$repo.settings");
    expect(settings.RepoSettingsPage ?? settings.default).toBeTruthy();
    // Source-level gate: settings must use can_admin, not owner_id equality.
    const settingsSrc = await import("./$owner.$repo.settings.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(settingsSrc).toMatch(/can_admin/);
    expect(settingsSrc).not.toMatch(/me\.id\s*===\s*repo\.owner_id/);
  }, 30_000);

  it("Collaborators section: list + Add collaborator empty state", async () => {
    const panel = await import("../components/repo/collaborators-panel");
    expect(panel.CollaboratorsPanel ?? panel.default).toBeTruthy();
    const src = await import("../components/repo/collaborators-panel.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/Collaborators/);
    expect(src).toMatch(/Add collaborator/);
    expect(src).toMatch(/No collaborators yet/);
  }, 30_000);

  it("add/update/remove permission ladder read | write | admin", async () => {
    const panel = await import("../components/repo/collaborators-panel");
    expect(panel.CollaboratorsPanel ?? panel.default).toBeTruthy();
    const src = await import("../components/repo/collaborators-panel.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/"read"/);
    expect(src).toMatch(/"write"/);
    expect(src).toMatch(/"admin"/);
    expect(src).toMatch(/collaborators\.add/);
    expect(src).toMatch(/collaborators\.update/);
    expect(src).toMatch(/collaborators\.remove/);
  }, 30_000);

  it("username lookup autocomplete never shows email (T-10-03)", async () => {
    const panel = await import("../components/repo/collaborators-panel");
    expect(panel.CollaboratorsPanel ?? panel.default).toBeTruthy();
    const src = await import("../components/repo/collaborators-panel.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/MemberLookup/);
    expect(src).toMatch(/kind:\s*"repo"/);
    expect(src).not.toMatch(/hit\.email/);
  }, 30_000);

  it("email invite panel: create list revoke + copy link", async () => {
    const src = await import("../components/repo/collaborators-panel.tsrx?raw").then((m) =>
      String((m as { default: string }).default),
    );
    expect(src).toMatch(/Email invitations/);
    expect(src).toMatch(/invites\.create/);
    expect(src).toMatch(/invites\.list/);
    expect(src).toMatch(/invites\.revoke/);
    expect(src).toMatch(/invite_url/);
    expect(src).toMatch(/Copy link/);
  }, 30_000);
});
