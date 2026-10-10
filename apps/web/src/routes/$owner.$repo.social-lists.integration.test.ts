import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const dir = dirname(fileURLToPath(import.meta.url));

describe("repo social list routes", () => {
  it("wires activity page with period/type filters and list SSR", () => {
    const src = readFileSync(join(dir, "$owner.$repo.activity.tsrx"), "utf8");
    expect(src).toMatch(/matchPath\("\/\$owner\/\$repo\/activity"/);
    expect(src).toMatch(/export function RepoActivityPage/);
    expect(src).toMatch(/fetchRepoActivity/);
    expect(src).toMatch(/period/);
    expect(src).toMatch(/force_push/);
    expect(src).toMatch(/branch_rename/);
    expect(src).toMatch(/Load more/);
  });

  it("wires stargazers page with Write+ gate, list SSR, and Load more", () => {
    const src = readFileSync(join(dir, "$owner.$repo.stargazers.tsrx"), "utf8");
    expect(src).toMatch(/matchPath\("\/\$owner\/\$repo\/stargazers"/);
    expect(src).toMatch(/export function RepoStargazersPage/);
    expect(src).toMatch(/fetchRepoStargazers/);
    expect(src).toMatch(/can_write/);
    expect(src).toMatch(/Find a stargazer/);
    expect(src).toMatch(/stargazersList/);
    expect(src).toMatch(/Load more/);
  });

  it("wires watchers page with list SSR, search, and Load more", () => {
    const src = readFileSync(join(dir, "$owner.$repo.watchers.tsrx"), "utf8");
    expect(src).toMatch(/matchPath\("\/\$owner\/\$repo\/watchers"/);
    expect(src).toMatch(/export function RepoWatchersPage/);
    expect(src).toMatch(/fetchRepoWatchers/);
    expect(src).toMatch(/Find a watcher/);
    expect(src).toMatch(/watchersList/);
    expect(src).toMatch(/Load more/);
  });

  it("wires forks page with sort, search, and Load more", () => {
    const src = readFileSync(join(dir, "$owner.$repo.forks.tsrx"), "utf8");
    expect(src).toMatch(/matchPath\("\/\$owner\/\$repo\/forks"/);
    expect(src).toMatch(/export function RepoForksPage/);
    expect(src).toMatch(/fetchRepoForks/);
    expect(src).toMatch(/parseForksSort/);
    expect(src).toMatch(/Find a fork/);
    expect(src).toMatch(/forksList/);
    expect(src).toMatch(/Load more/);
  });
});
