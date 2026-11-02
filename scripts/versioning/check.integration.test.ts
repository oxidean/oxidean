import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "../..");
const EXPECTED_ENTRIES = (readFileSync(join(ROOT, "versioning.toml"), "utf-8").match(/^\[\[component\]\]$/gm) ?? []).length;

function cliCheck(args: string[] = []) {
  return Bun.spawnSync(["bun", join(ROOT, "scripts/versioning/cli.ts"), "check", ...args], {
    cwd: ROOT,
    stdout: "pipe",
    stderr: "pipe",
  });
}

describe("cli check — real repository", () => {
  test("exits 0 and registers every component", () => {
    const proc = cliCheck();
    expect(proc.exitCode).toBe(0);
    expect(proc.stdout.toString()).toContain(`${EXPECTED_ENTRIES} components`);
  });

  test("--json emits structured violations/warnings/entries", () => {
    const proc = cliCheck(["--json"]);
    expect(proc.exitCode).toBe(0);
    const out = JSON.parse(proc.stdout.toString());
    expect(out.entries).toBe(EXPECTED_ENTRIES);
    expect(out.violations).toEqual([]);
    expect(Array.isArray(out.warnings)).toBe(true);
  });

  test("unknown argument → exit 2", () => {
    const proc = cliCheck(["--bogus"]);
    expect(proc.exitCode).toBe(2);
  });
});
