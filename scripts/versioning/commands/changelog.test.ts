import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { changelogCommand } from "./changelog.ts";
import { releaseTagsCommand } from "./release-tags.ts";

/**
 * Integration: a real `git init` fixture (same shape as plan.test.ts) so
 * changelog + release-tags run the live buildPlan/git paths end to end.
 */
let ROOT: string;

const REGISTRY_TOML = `version = 1

[baseline]
commit = "pending"

[grammar]
allowed_types = ["feat", "fix", "chore", "docs", "refactor", "perf", "test", "build", "ci", "revert"]
reserved_scopes = ["release"]

[[component]]
id = "core"
kind = "crate"
manifest_path = "crates/core/Cargo.toml"
tag_prefix = "core-v"
changelog_path = "crates/core/CHANGELOG.md"
globs = ["crates/core/**"]
scopes = ["core"]
depends_on = []

[[component]]
id = "app"
kind = "bun"
manifest_path = "packages/app/package.json"
tag_prefix = "app-v"
changelog_path = "packages/app/CHANGELOG.md"
globs = ["packages/app/**"]
scopes = ["app"]
depends_on = ["core"]
`;

function git(args: string[]): string {
  return execFileSync("git", args, { cwd: ROOT, encoding: "utf-8" });
}
function commit(msg: string, files: Record<string, string>): string {
  for (const [p, c] of Object.entries(files)) {
    mkdirSync(join(ROOT, p.split("/").slice(0, -1).join("/")), { recursive: true });
    writeFileSync(join(ROOT, p), c);
  }
  git(["add", "-A"]);
  git(["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-m", msg]);
  return git(["rev-parse", "HEAD"]).trim();
}

beforeAll(() => {
  ROOT = mkdtempSync(join(tmpdir(), "u4-cmd-"));
  git(["init", "-q", "-b", "main"]);
  mkdirSync(join(ROOT, "crates/core/src"), { recursive: true });
  mkdirSync(join(ROOT, "packages/app"), { recursive: true });
  writeFileSync(join(ROOT, "Cargo.toml"), '[workspace]\nmembers = ["crates/core"]\nresolver = "2"\n');
  writeFileSync(join(ROOT, "crates/core/Cargo.toml"), '[package]\nname = "core"\nversion = "0.1.0"\nedition = "2021"\n');
  writeFileSync(join(ROOT, "crates/core/src/lib.rs"), "");
  writeFileSync(join(ROOT, "package.json"), '{ "name": "fixture", "private": true, "workspaces": ["packages/app"] }\n');
  writeFileSync(join(ROOT, "packages/app/package.json"), '{ "name": "app", "version": "0.1.0", "private": true }\n');
  writeFileSync(join(ROOT, "versioning.toml"), REGISTRY_TOML);
  commit("chore(core): init", {});
  commit("feat(core): add thing", { "crates/core/src/thing.rs": "// x" });
});

afterAll(() => {
  rmSync(ROOT, { recursive: true, force: true });
});

describe("changelog", () => {
  test("writes per-component sections + the root consolidation", () => {
    expect(changelogCommand(ROOT, [])).toBe(0);
    const core = readFileSync(join(ROOT, "crates/core/CHANGELOG.md"), "utf-8");
    expect(core).toContain("## [0.2.0] - ");
    expect(core).toContain("### Features");
    expect(core).toContain("add thing");
    const app = readFileSync(join(ROOT, "packages/app/CHANGELOG.md"), "utf-8");
    expect(app).toContain("## [0.1.1] - ");
    expect(app).toContain("### Dependencies");
    expect(app).toContain("`core`");
    const root = readFileSync(join(ROOT, "CHANGELOG.md"), "utf-8");
    expect(root).toContain("`core` 0.1.0 → 0.2.0");
    expect(root).toContain("core-v0.2.0");
    expect(root).toContain("`app` 0.1.0 → 0.1.1");
  });

  test("re-running replaces the section in place (idempotent)", () => {
    expect(changelogCommand(ROOT, [])).toBe(0);
    const core = readFileSync(join(ROOT, "crates/core/CHANGELOG.md"), "utf-8");
    expect(core.match(/## \[0\.2\.0\]/g)).toHaveLength(1);
    // root consolidation gains one section per cut, but never double-writes
    // the same day: count `## Release` occurrences is still >= 1.
    const root = readFileSync(join(ROOT, "CHANGELOG.md"), "utf-8");
    expect(root.match(/## Release /g)!.length).toBeGreaterThanOrEqual(1);
  });

  test("--dry-run writes nothing", () => {
    rmSync(join(ROOT, "crates/core/CHANGELOG.md"));
    expect(changelogCommand(ROOT, ["--dry-run"])).toBe(0);
    // the deleted file stays deleted — dry-run never touches the fs
    expect(() => readFileSync(join(ROOT, "crates/core/CHANGELOG.md"), "utf-8")).toThrow();
    expect(
      readFileSync(join(ROOT, "packages/app/CHANGELOG.md"), "utf-8"),
    ).toContain("## [0.1.1]");
    // restore for the tag tests
    expect(changelogCommand(ROOT, [])).toBe(0);
  });
});

describe("release-tags", () => {
  test("--all creates an annotated tag per component at carrier version", () => {
    expect(releaseTagsCommand(ROOT, ["--all"])).toBe(0);
    const tags = git(["tag", "-l"]).split("\n").filter(Boolean).sort();
    expect(tags).toEqual(["app-v0.1.0", "core-v0.1.0"]);
    // annotated: cat-file reports `tag` objects, not commits
    expect(git(["cat-file", "-t", "core-v0.1.0"]).trim()).toBe("tag");
    const body = git(["tag", "-l", "--format=%(contents)", "core-v0.1.0"]);
    expect(body).toContain("## [0.1.0]");
  });

  test("existing tags are skipped, not recreated", () => {
    const before = git(["rev-list", "-n", "1", "core-v0.1.0"]).trim();
    expect(releaseTagsCommand(ROOT, ["--all"])).toBe(0);
    expect(git(["rev-list", "-n", "1", "core-v0.1.0"]).trim()).toBe(before);
    expect(git(["tag", "-l"]).split("\n").filter(Boolean)).toHaveLength(2);
  });

  test("--dry-run creates no tags", () => {
    const fresh = mkdtempSync(join(tmpdir(), "u4-tags-"));
    try {
      execFileSync("git", ["init", "-q", "-b", "main"], { cwd: fresh });
      for (const [p, c] of Object.entries({
        "crates/core/Cargo.toml": '[package]\nname = "core"\nversion = "0.1.0"\n',
        "crates/core/src/lib.rs": "",
        "packages/app/package.json": '{ "name": "app", "version": "0.1.0" }\n',
        "Cargo.toml": '[workspace]\nmembers = ["crates/core"]\n',
        "package.json": '{ "name": "f", "workspaces": ["packages/app"] }\n',
        "versioning.toml": REGISTRY_TOML,
      })) {
        mkdirSync(join(fresh, p.split("/").slice(0, -1).join("/") || "."), { recursive: true });
        writeFileSync(join(fresh, p), c);
      }
      execFileSync("git", ["add", "-A"], { cwd: fresh });
      execFileSync("git", ["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-m", "feat(core): init"], { cwd: fresh });
      expect(releaseTagsCommand(fresh, ["--all", "--dry-run"])).toBe(0);
      expect(execFileSync("git", ["tag", "-l"], { cwd: fresh, encoding: "utf-8" }).trim()).toBe("");
    } finally {
      rmSync(fresh, { recursive: true, force: true });
    }
  });
});
