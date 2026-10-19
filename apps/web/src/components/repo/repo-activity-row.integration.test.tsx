/**
 * RepoActivityRow — `vcs.*` kinds render human-readable copy, and `cs_<ulid>`
 * changeset ids never route to `/commit/{sha}` (they are not git oids — the
 * link would 404). `vcs.git_export` rows carry real git oids and DO link.
 */
import { cleanup } from "@octanejs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import type { RepoActivityItem } from "@oxidean/api-client";
import { RepoActivityRow } from "@/components/repo/repo-activity-row";
import { renderWithQueryClient } from "@/test/render-with-query";

const GIT_OID = "0123456789abcdef0123456789abcdef01234567";
const CS_ID = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR";

function item(overrides: Partial<RepoActivityItem>): RepoActivityItem {
  return {
    id: "a1",
    push_type: "push",
    ref_name: "main",
    ref_short: "main",
    before: null,
    after: null,
    pushed_at: "2026-10-19T00:00:00Z",
    commits_count: null,
    commit_message: null,
    pusher: { login: "ada", name: "Ada", path: "/ada" },
    ...overrides,
  };
}

function renderRow(row: RepoActivityItem): HTMLElement {
  const { container } = renderWithQueryClient(RepoActivityRow, {
    props: { owner: "ada", repoName: "hello", item: row },
  });
  return container;
}

function commitLinks(container: HTMLElement): HTMLAnchorElement[] {
  return Array.from(container.querySelectorAll('a[href*="/commit/"]'));
}

afterEach(cleanup);

describe("RepoActivityRow vcs.* rendering", () => {
  it("vcs.changeset_create renders copy and NO commit link for cs_ ids", () => {
    const container = renderRow(
      item({
        push_type: "vcs.changeset_create",
        ref_name: null,
        ref_short: null,
        after: CS_ID,
        commits_count: 1,
        commit_message: "add parser",
      }),
    );
    expect(container.textContent).toContain("created a VCS changeset");
    expect(container.textContent).toContain("add parser");
    expect(commitLinks(container)).toHaveLength(0);
  });

  it("vcs.git_export links its real git oid to /commit/", () => {
    const container = renderRow(
      item({
        push_type: "vcs.git_export",
        ref_name: "refs/heads/main",
        ref_short: "main",
        after: GIT_OID,
        commits_count: 3,
        commit_message: "export batch",
      }),
    );
    expect(container.textContent).toContain("exported VCS to Git ref main");
    const links = commitLinks(container);
    expect(links).toHaveLength(1);
    expect(links[0]!.href).toContain(`/commit/${GIT_OID}`);
  });

  it("vcs.ref_update renders create/update/delete variants", () => {
    const created = renderRow(
      item({
        push_type: "vcs.ref_update",
        ref_name: "agent/work",
        ref_short: "agent/work",
        before: null,
        after: CS_ID,
      }),
    );
    expect(created.textContent).toContain("created VCS ref agent/work");
    cleanup();

    const deleted = renderRow(
      item({
        push_type: "vcs.ref_update",
        ref_name: "agent/work",
        ref_short: "agent/work",
        before: CS_ID,
        after: null,
      }),
    );
    expect(deleted.textContent).toContain("deleted VCS ref agent/work");
    cleanup();

    const updated = renderRow(
      item({
        push_type: "vcs.ref_update",
        ref_name: "agent/work",
        ref_short: "agent/work",
        before: CS_ID,
        after: "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQS",
      }),
    );
    expect(updated.textContent).toContain("updated VCS ref agent/work");
  });

  it("vcs.repo_enable renders flag copy with no commit link", () => {
    const container = renderRow(
      item({
        push_type: "vcs.repo_enable",
        ref_name: null,
        ref_short: null,
      }),
    );
    expect(container.textContent).toContain("updated the VCS protocol flag");
    expect(commitLinks(container)).toHaveLength(0);
  });

  it("vcs.merge / vcs.rebase / vcs.git_import render their copy", () => {
    for (const [pushType, copy] of [
      ["vcs.merge", "merged VCS changesets"],
      ["vcs.rebase", "rebased a VCS changeset"],
      ["vcs.git_import", "imported Git history into VCS"],
    ] as const) {
      const container = renderRow(item({ push_type: pushType, after: CS_ID }));
      expect(container.textContent).toContain(copy);
      expect(commitLinks(container)).toHaveLength(0);
      cleanup();
    }
  });

  it("git push regression: 40-hex after still links", () => {
    const container = renderRow(
      item({
        push_type: "push",
        after: GIT_OID,
        commits_count: 2,
        commit_message: "fix thing",
      }),
    );
    const links = commitLinks(container);
    expect(links).toHaveLength(1);
    expect(links[0]!.href).toContain(`/commit/${GIT_OID}`);
  });
});
