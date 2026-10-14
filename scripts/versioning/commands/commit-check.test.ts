import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { checkMessages, commitCheckCommand } from "./commit-check.ts";
import type { RegistryConfig } from "../lib/registry.ts";

const registry: RegistryConfig = {
  version: 1,
  baseline_commit: "pending",
  allowed_types: ["feat", "fix", "chore", "docs", "style", "refactor", "perf", "test", "build", "ci", "revert"],
  reserved_scopes: ["release"],
  entries: [
    {
      id: "oxidean-core",
      kind: "crate",
      manifest_path: "crates/oxidean-core/Cargo.toml",
      tag_prefix: "oxidean-core-v",
      changelog_path: "crates/oxidean-core/CHANGELOG.md",
      globs: ["crates/oxidean-core/**"],
      scopes: ["core"],
      depends_on: [],
    },
  ],
};

const tmp: string[] = [];
afterEach(() => {
  while (tmp.length) rmSync(tmp.pop()!, { recursive: true, force: true });
});

describe("checkMessages", () => {
  test("valid message → exit 0 with ok line", () => {
    const r = checkMessages(registry, [{ subject: "feat(core): x" }]);
    expect(r.exitCode).toBe(0);
    expect(r.lines[0]).toContain("ok");
  });

  test("invalid message → exit 1 naming the violation", () => {
    const r = checkMessages(registry, [{ subject: "bogus(core): x" }]);
    expect(r.exitCode).toBe(1);
    expect(r.lines[0]).toContain("unknown type");
  });

  test("mixed range → exit 1; exempt commits reported ok (exempt)", () => {
    const r = checkMessages(registry, [
      { sha: "a".repeat(40), subject: "feat(core): x" },
      { sha: "b".repeat(40), subject: "Merge branch x" },
      { sha: "c".repeat(40), subject: "totally invalid" },
    ]);
    expect(r.exitCode).toBe(1);
    expect(r.lines[1]).toContain("ok (exempt)");
    expect(r.lines[2]).toContain(`commit ${"c".repeat(10)}`);
  });
});

describe("commitCheckCommand — modes", () => {
  // Run against the real registry file so argv paths exercise loadRegistry.
  const root = join(import.meta.dir, "../../..");

  test("--file parses subject + body together", () => {
    const dir = mkdtempSync(join(tmpdir(), "cc-"));
    tmp.push(dir);
    const f = join(dir, "COMMIT_EDITMSG");
    writeFileSync(f, "feat(core): x\n\nBREAKING CHANGE: api dropped\n");
    const code = commitCheckCommand(root, ["--file", f]);
    expect(code).toBe(0);
  });

  test("no mode flag → exit 2", () => {
    expect(commitCheckCommand(root, [])).toBe(2);
    expect(commitCheckCommand(root, ["--bogus", "x"])).toBe(2);
  });
});
