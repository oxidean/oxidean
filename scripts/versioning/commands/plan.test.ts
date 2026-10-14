import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { detectCommand } from "./detect.ts";
import { planCommand } from "./plan.ts";
import { bumpCommand } from "./bump.ts";

/**
 * Boundary integration: a real `git init` fixture repo holding a tiny Cargo
 * workspace + one Bun package so detect/plan/bump run the live git, glob,
 * carrier, and regen paths end to end.
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
  ROOT = mkdtempSync(join(tmpdir(), "u3-cmd-"));
  git(["init", "-q", "-b", "main"]);
  mkdirSync(join(ROOT, "crates/core"), { recursive: true });
  mkdirSync(join(ROOT, "packages/app"), { recursive: true });
  writeFileSync(
    join(ROOT, "Cargo.toml"),
    '[workspace]\nmembers = ["crates/core"]\nresolver = "2"\n',
  );
  writeFileSync(
    join(ROOT, "crates/core/Cargo.toml"),
    '[package]\nname = "core"\nversion = "0.1.0"\nedition = "2021"\n',
  );
  mkdirSync(join(ROOT, "crates/core/src"), { recursive: true });
  writeFileSync(join(ROOT, "crates/core/src/lib.rs"), "");
  writeFileSync(
    join(ROOT, "package.json"),
    '{ "name": "fixture", "private": true, "workspaces": ["packages/app"] }\n',
  );
  writeFileSync(
    join(ROOT, "packages/app/package.json"),
    '{ "name": "app", "version": "0.1.0", "private": true }\n',
  );
  writeFileSync(join(ROOT, "versioning.toml"), REGISTRY_TOML);
  commit("chore(core): init", {});
});

afterAll(() => {
  rmSync(ROOT, { recursive: true, force: true });
});

describe("detect", () => {
  test("attributes a feat commit by scope and globs", () => {
    commit("feat(core): add thing", { "crates/core/src/thing.rs": "// x" });
    const rc = detectCommand(ROOT, ["--json"]);
    expect(rc).toBe(0);
  });
});

describe("plan", () => {
  test("plan --json emits the C3 payload", () => {
    let out = "";
    const orig = process.stdout.write;
    process.stdout.write = ((s: string) => ((out += s), true)) as typeof orig;
    try {
      expect(planCommand(ROOT, ["--json"])).toBe(0);
    } finally {
      process.stdout.write = orig;
    }
    const plan = JSON.parse(out);
    expect(plan.schema).toBe("release-plan");
    expect(plan.version).toBe(1);
    const core = plan.entries.find((e: { component: string }) => e.component === "core");
    const app = plan.entries.find((e: { component: string }) => e.component === "app");
    expect(core.bump).toBe("minor");
    expect(core.toVersion).toBe("0.2.0");
    // app has no direct commits but depends on core → propagated patch.
    expect(app.bump).toBe("patch");
    expect(app.propagatedFrom).toEqual(["core"]);
  });

  test("--since bounds the range", () => {
    const head = git(["rev-parse", "HEAD"]).trim();
    let out = "";
    const orig = process.stdout.write;
    process.stdout.write = ((s: string) => ((out += s), true)) as typeof orig;
    try {
      expect(planCommand(ROOT, ["--json", "--since", head])).toBe(0);
    } finally {
      process.stdout.write = orig;
    }
    expect(JSON.parse(out).entries).toEqual([]);
  });
});

describe("bump", () => {
  test("--dry-run writes nothing", () => {
    const before = readFileSync(join(ROOT, "crates/core/Cargo.toml"), "utf-8");
    expect(bumpCommand(ROOT, ["--dry-run"])).toBe(0);
    expect(readFileSync(join(ROOT, "crates/core/Cargo.toml"), "utf-8")).toBe(before);
  });

  test("applies manifest writes + regenerates lockfiles", () => {
    expect(bumpCommand(ROOT, [])).toBe(0);
    expect(readFileSync(join(ROOT, "crates/core/Cargo.toml"), "utf-8")).toContain(
      'version = "0.2.0"',
    );
    expect(readFileSync(join(ROOT, "packages/app/package.json"), "utf-8")).toContain(
      '"version": "0.1.1"',
    );
    // cargo metadata regen kept Cargo.lock in sync.
    const lock = readFileSync(join(ROOT, "Cargo.lock"), "utf-8");
    expect(lock).toContain('version = "0.2.0"');
  });
});
