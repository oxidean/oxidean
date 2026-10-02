import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * Phase 12 Pulls UI — tracer greened chrome/list/new/detail; later plans green the rest.
 */

const chromeActive = readFileSync(join(process.cwd(), "src/lib/repo-chrome-active.ts"), "utf8");
const repoChrome = readFileSync(
  join(process.cwd(), "src/components/repo/repo-chrome.tsrx"),
  "utf8",
);
const pullsIndex = readFileSync(
  join(process.cwd(), "src/routes/$owner.$repo.pulls.index.tsrx"),
  "utf8",
);
const pullsNew = readFileSync(
  join(process.cwd(), "src/routes/$owner.$repo.pulls.new.tsrx"),
  "utf8",
);
const pullDetail = readFileSync(
  join(process.cwd(), "src/routes/$owner.$repo.pull.$n.tsrx"),
  "utf8",
);

describe("Phase 12 Pulls UI", () => {
  it("RepoChromeActive includes pulls and maps /pulls|/pull", () => {
    expect(chromeActive).toMatch(/"pulls"/);
    expect(chromeActive).toMatch(/pulls:\s*"pulls"/);
    expect(chromeActive).toMatch(/pull:\s*"pulls"/);
  });

  it("RepoChrome renders Pulls tab", () => {
    expect(repoChrome).toMatch(/Pulls/);
    expect(repoChrome).toMatch(/\/pulls/);
  });

  it("pulls list route defaults Open with Closed/All", () => {
    expect(pullsIndex).toMatch(/Open/);
    expect(pullsIndex).toMatch(/Closed/);
    expect(pullsIndex).toMatch(/All/);
  });

  it("New pull request gated by can_write", () => {
    expect(pullsIndex).toMatch(/can_write/);
    expect(pullsNew).toMatch(/Create pull request/);
  });

  it("Author filter uses MemberLookup autocomplete", () => {
    expect(pullsIndex).toMatch(/MemberLookup/);
    expect(pullsIndex).toMatch(/pull-filter-author/);
  });

  it("pulls list filters include assignee and search", () => {
    expect(pullsIndex).toMatch(/pull-filter-assignee/);
    expect(pullsIndex).toMatch(/Search title or body/);
    expect(pullsIndex).toMatch(/assignee:/);
    expect(pullsIndex).toMatch(/\bq:/);
  });

  it("new PR head owner uses MemberLookup", () => {
    expect(pullsNew).toMatch(/MemberLookup/);
    expect(pullsNew).toMatch(/pull-new-head-owner/);
  });

  it("pulls list rows show assignees when present", () => {
    const pullsList = readFileSync(
      join(process.cwd(), "src/components/repo/pulls-list.tsrx"),
      "utf8",
    );
    expect(pullsList).toMatch(/assigned/);
    expect(pullsList).toMatch(/assignees/);
  });

  it("compare flow can create a PR", () => {
    const compare = readFileSync(
      join(process.cwd(), "src/routes/$owner.$repo.compare.$.tsrx"),
      "utf8",
    );
    expect(compare).toMatch(/Create pull request/);
    expect(compare).toMatch(/pulls\/new/);
  });

  it("detail tabs Conversation | Commits | Files changed", () => {
    expect(pullDetail).toMatch(/Conversation/);
    expect(pullDetail).toMatch(/Commits/);
    expect(pullDetail).toMatch(/Files changed/);
  });

  it("unified and split diff toggle", () => {
    const pullFiles = readFileSync(
      join(process.cwd(), "src/components/repo/pull-files.tsrx"),
      "utf8",
    );
    expect(pullFiles).toMatch(/Unified/);
    expect(pullFiles).toMatch(/Split/);
  });

  it("review actions Approve / Request changes / Comment", () => {
    const reviews = readFileSync(
      join(process.cwd(), "src/components/repo/pull-reviews.tsrx"),
      "utf8",
    );
    expect(reviews).toMatch(/Approve/);
    expect(reviews).toMatch(/Request changes/);
    expect(reviews).toMatch(/Comment/);
  });

  it("merge method picker + close/reopen", () => {
    expect(pullDetail).toMatch(/Close pull request/);
    expect(pullDetail).toMatch(/Reopen pull request/);
    expect(pullDetail).toMatch(/PullMergePanel/);
  });

  it("Admin merge strategy settings", () => {
    const settings = readFileSync(
      join(process.cwd(), "src/components/repo/merge-settings-panel.tsrx"),
      "utf8",
    );
    expect(settings).toMatch(/Allow merge commits/);
    expect(settings).toMatch(/Allow squash merging/);
    expect(settings).toMatch(/Allow rebase merging/);
  });

  it("Write|Preview on PR comments", () => {
    const conversation = readFileSync(
      join(process.cwd(), "src/components/repo/pull-conversation.tsrx"),
      "utf8",
    );
    expect(conversation).toMatch(/MarkdownWritePreview/);
    expect(conversation).toMatch(/Outdated/);
  });
});
