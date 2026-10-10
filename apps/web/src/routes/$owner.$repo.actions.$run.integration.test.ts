/**
 * Phase 19 — Actions run detail + job logs (ACT-03 / D-ACT-12 / D-ACT-13).
 */
import { describe, expect, it } from "vitest";

describe("/$owner/$repo/actions/$run", () => {
  it("shows run detail with job list and statuses", async () => {
    const src = await import("./$owner.$repo.actions.$run.tsrx?raw").then(
      (m) => m.default as string,
    );
    expect(src).toContain('data-testid="repo-actions-run"');
    expect(src).toContain("repo-actions-jobs");
    expect(src).toContain("actionsRunDetailQuery");
  });

  it("renders the step-aware job log viewer from the Actions log query", async () => {
    const src = await import("./$owner.$repo.actions.$run.tsrx?raw").then(
      (m) => m.default as string,
    );
    expect(src).toContain("ActionsLogViewer");
    expect(src).toContain("actionsJobLogQuery");
    const viewer = await import("../components/repo/actions-log-viewer.tsrx?raw").then(
      (m) => m.default as string,
    );
    expect(viewer).toContain("repo-actions-job-log");
    expect(viewer).toContain("actions-log-steps");
    expect(viewer).toContain("actions-log-search");
  });

  it("inherits layout chrome — no duplicate RepoChrome remount (D-QH-01)", async () => {
    const src = await import("./$owner.$repo.actions.$run.tsrx?raw").then(
      (m) => m.default as string,
    );
    expect(src).not.toMatch(/RepoChrome/);
    expect(src).toContain('matchPath("/$owner/$repo/actions/$run"');
  });
});
