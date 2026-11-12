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

  test("--file cuts below a custom commentChar scissors line", () => {
    const dir = mkdtempSync(join(tmpdir(), "cc-"));
    tmp.push(dir);
    const f = join(dir, "COMMIT_EDITMSG");
    writeFileSync(
      f,
      "feat(core): x\n% ------------------------ >8 ------------------------\nGenerated with Cursor\n",
    );
    // The post-marker text is not message text; without honoring the
    // custom prefix it would be scanned and fail the gate.
    const code = commitCheckCommand(root, ["--file", f], {
      commentPrefix: () => "%",
    });
    expect(code).toBe(0);
  });

  test("--file skips leading comment lines for the subject", () => {
    const dir = mkdtempSync(join(tmpdir(), "cc-"));
    tmp.push(dir);
    const f = join(dir, "COMMIT_EDITMSG");
    writeFileSync(f, "% template header\nfeat(core): x\n");
    const code = commitCheckCommand(root, ["--file", f], {
      commentPrefix: () => "%",
    });
    expect(code).toBe(0);
  });

  test("no mode flag → exit 2", () => {
    expect(commitCheckCommand(root, [])).toBe(2);
    expect(commitCheckCommand(root, ["--bogus", "x"])).toBe(2);
  });
});

describe("attribution scan", () => {
  const root = join(import.meta.dir, "../../..");

  test("AI Co-authored-by trailer in body → exit 1", () => {
    const r = checkMessages(registry, [
      {
        subject: "feat(core): x",
        body: "\nCo-Authored-By: Claude <noreply@anthropic.com>\n",
      },
    ]);
    expect(r.exitCode).toBe(1);
    expect(r.failures[0].error).toContain("attribution: AI Co-authored-by trailer");
  });

  test("human Co-authored-by stays clean", () => {
    const r = checkMessages(registry, [
      {
        subject: "feat(core): x",
        body: "\nCo-authored-by: Jane Doe <jane@example.com>\n",
      },
    ]);
    expect(r.exitCode).toBe(0);
  });

  test("human co-author with vendor-token name stays clean", () => {
    const r = checkMessages(registry, [
      {
        subject: "feat(core): x",
        body: "\nCo-authored-by: Claude Martin <claude.martin@example.com>\n",
      },
    ]);
    expect(r.exitCode).toBe(0);
  });

  test("vendor display name without email still fails", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nCo-authored-by: Claude\n" },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("bot/[bot] trailer fails on machine signals", () => {
    const r = checkMessages(registry, [
      {
        subject: "feat(core): x",
        body: "\nCo-authored-by: devin-ai-integration[bot] <158+x[bot]@users.noreply.github.com>\n",
      },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("human surname ending in bot stays clean", () => {
    const r = checkMessages(registry, [
      {
        subject: "feat(core): x",
        body: "\nCo-authored-by: Alice Talbot <alice.talbot@example.org>\n",
      },
    ]);
    expect(r.exitCode).toBe(0);
  });

  test("multiword vendor display name fails", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nCo-authored-by: GitHub Copilot\n" },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("model-tier vendor display name fails", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nCo-authored-by: Claude Sonnet\n" },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("vendor domain without closing bracket fails", () => {
    const r = checkMessages(registry, [
      {
        subject: "feat(core): x",
        body: "\nCo-authored-by: x <bot@anthropic.com\n",
      },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("via-line with ' AI' suffix on any vendor fails", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nvia Claude AI\n" },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("generator footer accepts a colon after the connector", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nCreated with: Cursor\n" },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("vendor token needs a name boundary — 'cursors' stays clean", () => {
    const r = checkMessages(registry, [
      { subject: "fix(core): handle events generated by cursors" },
    ]);
    expect(r.exitCode).toBe(0);
  });

  test("mid-sentence robot emoji stays clean", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): add 🤖 status icon" },
    ]);
    expect(r.exitCode).toBe(0);
  });

  test("bare robot-emoji signature line fails", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\n🤖\n" },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("robot emoji beside co-author vocabulary fails", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nCo-authored-by: x 🤖 <x@y.z>\n" },
    ]);
    expect(r.exitCode).toBe(1);
  });

  test("via-line with trailing punctuation fails", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nvia Cursor.\n" },
      { subject: "feat(core): y", body: "\nvia devin-ai\n" },
    ]);
    expect(r.exitCode).toBe(1);
    expect(r.failures.map((f) => f.error).join("\n")).toContain(
      "attribution: attribution via-line",
    );
  });

  test("standalone via-line footer fails", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nvia Cursor\n" },
    ]);
    expect(r.exitCode).toBe(1);
    expect(r.failures[0].error).toContain("attribution: attribution via-line");
  });

  test("prose containing a vendor word stays clean", () => {
    const r = checkMessages(registry, [
      {
        subject: "feat(core): x",
        body: "\ninstalled via cursor extension, see docs\n",
      },
    ]);
    expect(r.exitCode).toBe(0);
  });

  test("generator footer and zero-width marker both fail", () => {
    const r = checkMessages(registry, [
      { subject: "feat(core): x", body: "\nGenerated with Cursor\n" },
      { subject: "feat(core): y", body: "invisible\u200Bmarker" },
    ]);
    expect(r.exitCode).toBe(1);
    expect(r.failures.map((f) => f.error).join("\n")).toContain(
      "AI generator footer",
    );
    expect(r.failures.map((f) => f.error).join("\n")).toContain(
      "zero-width/invisible attribution marker",
    );
  });

  test("--scan-file lints attribution only, no grammar", () => {
    const dir = mkdtempSync(join(tmpdir(), "cc-scan-"));
    tmp.push(dir);
    const clean = join(dir, "clean.txt");
    const dirty = join(dir, "dirty.txt");
    writeFileSync(clean, "not even a commit subject\n");
    writeFileSync(dirty, "body\n\nMade-with: Copilot\n");
    expect(commitCheckCommand(root, ["--scan-file", clean])).toBe(0);
    expect(commitCheckCommand(root, ["--scan-file", dirty])).toBe(1);
  });

  test("--file cuts the diff below the scissors line", () => {
    const dir = mkdtempSync(join(tmpdir(), "cc-"));
    tmp.push(dir);
    const f = join(dir, "COMMIT_EDITMSG");
    // Marker inside the -v diff must not fail; marker in the message must.
    writeFileSync(
      f,
      "feat(core): x\n\n# ------------------------ >8 ------------------------\n+Co-Authored-By: Claude <noreply@anthropic.com>\n",
    );
    expect(commitCheckCommand(root, ["--file", f])).toBe(0);
    writeFileSync(
      f,
      "feat(core): x\n\nCo-Authored-By: Claude <noreply@anthropic.com>\n# ------------------------ >8 ------------------------\n",
    );
    expect(commitCheckCommand(root, ["--file", f])).toBe(1);
  });
});
