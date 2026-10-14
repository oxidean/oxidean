/**
 * U5 enforcement-surface tests — real `git commit` through `.githooks/`,
 * `make setup` behaviour in a temp repo, and static ci.yml shape assertions.
 */
import { describe, expect, test } from "bun:test";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..", "..");

function git(repo: string, args: string[]): { code: number; out: string; err: string } {
  const p = Bun.spawnSync(["git", ...args], {
    cwd: repo,
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@t", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@t" },
  });
  return { code: p.exitCode, out: p.stdout.toString(), err: p.stderr.toString() };
}

/** Temp git repo that resolves the real repo's versioning CLI via symlinks. */
function fixtureRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "u5-hook-"));
  git(repo, ["init", "-q"]);
  mkdirSync(join(repo, "scripts"), { recursive: true });
  symlinkSync(join(ROOT, "scripts", "versioning"), join(repo, "scripts", "versioning"));
  symlinkSync(join(ROOT, "versioning.toml"), join(repo, "versioning.toml"));
  symlinkSync(join(ROOT, ".githooks"), join(repo, ".githooks"));
  writeFileSync(join(repo, "f.txt"), "x\n");
  git(repo, ["add", "."]);
  return repo;
}

describe("commit-msg hook", () => {
  test("accepts a grammar-valid commit", () => {
    const repo = fixtureRepo();
    git(repo, ["config", "core.hooksPath", ".githooks"]);
    const r = git(repo, ["commit", "-q", "-m", "feat(core): add thing", "--allow-empty"]);
    expect(r.code).toBe(0);
  });

  test("rejects an invalid subject with the parser's failure line", () => {
    const repo = fixtureRepo();
    git(repo, ["config", "core.hooksPath", ".githooks"]);
    const r = git(repo, ["commit", "-m", "no prefix here", "--allow-empty"]);
    expect(r.code).toBe(1);
    expect(r.err + r.out).toMatch(/commit|type|grammar|scope/i);
  });

  test("is executable on disk and survives a subdirectory commit", () => {
    const mode = statSync(join(ROOT, ".githooks", "commit-msg")).mode;
    expect(mode & 0o111).not.toBe(0);

    const repo = fixtureRepo();
    git(repo, ["config", "core.hooksPath", ".githooks"]);
    mkdirSync(join(repo, "sub"), { recursive: true });
    writeFileSync(join(repo, "sub", "f.txt"), "y\n");
    git(repo, ["add", "."]);
    const r = git(join(repo, "sub"), ["commit", "-q", "-m", "fix(app): patch", "--allow-empty"]);
    expect(r.code).toBe(0);
  });

  test("--no-verify bypasses the hook", () => {
    const repo = fixtureRepo();
    git(repo, ["config", "core.hooksPath", ".githooks"]);
    const r = git(repo, ["commit", "-q", "-m", "no prefix", "--allow-empty", "--no-verify"]);
    expect(r.code).toBe(0);
  });
});

describe("make setup", () => {
  function makeSetup(repo: string): { code: number; out: string; err: string } {
    const p = Bun.spawnSync(["make", "-f", join(ROOT, "Makefile"), "setup"], {
      cwd: repo,
      stdout: "pipe",
      stderr: "pipe",
    });
    return { code: p.exitCode, out: p.stdout.toString(), err: p.stderr.toString() };
  }

  test("sets core.hooksPath and warns on a foreign existing value", () => {
    const repo = fixtureRepo();
    git(repo, ["config", "core.hooksPath", "custom/hooks"]);
    const r = makeSetup(repo);
    expect(r.code).toBe(0);
    expect(r.err).toMatch(/custom\/hooks/);
    expect(git(repo, ["config", "--get", "core.hooksPath"]).out.trim()).toBe(".githooks");

    // Idempotent re-run: no warning once .githooks is installed.
    const again = makeSetup(repo);
    expect(again.code).toBe(0);
    expect(again.err).not.toMatch(/replacing/);
  });
});

describe("ci.yml commit-lint job", () => {
  const ci = readFileSync(join(ROOT, ".github", "workflows", "ci.yml"), "utf-8");
  const job = ci.slice(ci.indexOf("  commit-lint:"));

  test("job exists with full history checkout and read-only permissions", () => {
    expect(ci).toContain("commit-lint:");
    expect(job).toContain("fetch-depth: 0");
    expect(job).toContain("contents: read");
  });

  test("lints the PR range, push range, and PR title", () => {
    expect(job).toContain("commit-check --range");
    expect(job).toContain("PR_BASE..$PR_HEAD");
    expect(job).toContain("PUSH_BEFORE..$PUSH_AFTER");
    expect(job).toContain("commit-check --pr-title");
    expect(job).toContain("github.event_name == 'pull_request'");
  });

  test("runs the registry completeness check", () => {
    expect(job).toContain("cli.ts check");
  });
});
