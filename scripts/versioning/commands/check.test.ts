import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCheck } from "./check.ts";
import type { WorkspaceMembers } from "../lib/workspace.ts";

const tmp: string[] = [];
afterEach(() => {
  while (tmp.length) rmSync(tmp.pop()!, { recursive: true, force: true });
});

/** Minimal fixture workspace: one crate + one bun pkg + the wire contract. */
function fixture(): { root: string; members: WorkspaceMembers } {
  const root = mkdtempSync(join(tmpdir(), "ver-"));
  tmp.push(root);
  mkdirSync(join(root, "crates/a"), { recursive: true });
  writeFileSync(join(root, "crates/a/Cargo.toml"), '[package]\nname = "a"\nversion = "0.1.0"\n');
  mkdirSync(join(root, "apps/b"), { recursive: true });
  writeFileSync(join(root, "apps/b/package.json"), '{"name":"b","version":"0.1.0"}');
  mkdirSync(join(root, "crates/core/src"), { recursive: true });
  writeFileSync(
    join(root, "crates/core/src/lib.rs"),
    'pub const WIRE_VERSION: &str = "0.1.0";\n',
  );
  writeFileSync(join(root, "package.json"), '{"name":"root","private":true,"workspaces":["apps/*"]}');
  writeFileSync(
    join(root, "versioning.toml"),
    `version = 1
[baseline]
commit = "pending"
[grammar]
allowed_types = ["feat"]
reserved_scopes = ["release"]
[[component]]
id = "a"
kind = "crate"
manifest_path = "crates/a/Cargo.toml"
tag_prefix = "a-v"
changelog_path = "crates/a/CHANGELOG.md"
globs = ["crates/a/**"]
scopes = ["a"]
depends_on = []
[[component]]
id = "b"
kind = "bun"
manifest_path = "apps/b/package.json"
tag_prefix = "b-v"
changelog_path = "apps/b/CHANGELOG.md"
globs = ["apps/b/**"]
scopes = ["b"]
depends_on = []
[[component]]
id = "wire"
kind = "contract"
constant_path = "crates/core/src/lib.rs#WIRE_VERSION"
tag_prefix = "wire-v"
changelog_path = "docs/WIRE-CHANGELOG.md"
globs = []
scopes = ["wire"]
depends_on = []
`,
  );
  return {
    root,
    members: { cargo: ["crates/a/Cargo.toml"], bun: ["apps/b/package.json"] },
  };
}

const injected = (members: WorkspaceMembers) => ({ enumerate: () => members });

describe("runCheck — fixture workspace", () => {
  test("complete registry → exit-clean result + pending warning", () => {
    const { root, members } = fixture();
    const r = runCheck(root, injected(members));
    expect(r.violations).toEqual([]);
    expect(r.entries).toBe(3);
    expect(r.warnings.some((w) => w.includes('"pending"'))).toBe(true);
  });

  test("workspace member with no entry → violation (BR1.2)", () => {
    const { root } = fixture();
    const members: WorkspaceMembers = {
      cargo: ["crates/a/Cargo.toml", "crates/unregistered/Cargo.toml"],
      bun: ["apps/b/package.json"],
    };
    const r = runCheck(root, injected(members));
    expect(r.violations.some((v) => v.message.includes("crates/unregistered"))).toBe(true);
  });

  test("entry pointing at non-member manifest → violation (BR1.2)", () => {
    const { root } = fixture();
    const members: WorkspaceMembers = { cargo: [], bun: ["apps/b/package.json"] };
    const r = runCheck(root, injected(members));
    expect(r.violations.some((v) => v.message.includes('"a"') && v.message.includes("not a workspace member"))).toBe(true);
  });

  test("workspace-inherited crate version → violation (BR1.1)", () => {
    const { root, members } = fixture();
    writeFileSync(join(root, "crates/a/Cargo.toml"), '[package]\nname = "a"\nversion.workspace = true\n');
    const r = runCheck(root, injected(members));
    expect(r.violations.some((v) => v.message.includes("literal semver"))).toBe(true);
  });

  test("manifest under a non-component path → violation (BR1.5)", () => {
    const { root, members } = fixture();
    const text = readFileSync(join(root, "versioning.toml"), "utf-8").replace(
      'manifest_path = "crates/a/Cargo.toml"',
      'manifest_path = "crates/x/templates/t/Cargo.toml"',
    );
    writeFileSync(join(root, "versioning.toml"), text);
    const r = runCheck(root, injected(members));
    expect(r.violations.some((v) => v.message.includes("non-component"))).toBe(true);
  });

  test("missing constant in source → violation (BR1.1)", () => {
    const { root, members } = fixture();
    writeFileSync(join(root, "crates/core/src/lib.rs"), "// no version here\n");
    const r = runCheck(root, injected(members));
    expect(r.violations.some((v) => v.message.includes("WIRE_VERSION"))).toBe(true);
  });

  /** Fixture + a second crate member so path-dep resolution has a target. */
  function withSecondCrate(root: string) {
    mkdirSync(join(root, "crates/b"), { recursive: true });
    writeFileSync(join(root, "crates/b/Cargo.toml"), '[package]\nname = "bc"\nversion = "0.1.0"\n');
    writeFileSync(
      join(root, "versioning.toml"),
      readFileSync(join(root, "versioning.toml"), "utf-8") +
        `\n[[component]]\nid = "bc"\nkind = "crate"\nmanifest_path = "crates/b/Cargo.toml"\ntag_prefix = "bc-v"\nchangelog_path = "crates/b/CHANGELOG.md"\nglobs = ["crates/b/**"]\nscopes = ["bc"]\ndepends_on = []\n`,
    );
    return {
      cargo: ["crates/a/Cargo.toml", "crates/b/Cargo.toml"],
      bun: ["apps/b/package.json"],
    } satisfies WorkspaceMembers;
  }

  test("versioned path dep on workspace member → violation", () => {
    const { root } = fixture();
    const members = withSecondCrate(root);
    writeFileSync(
      join(root, "crates/a/Cargo.toml"),
      '[package]\nname = "a"\nversion = "0.1.0"\n\n[dependencies]\nbc = { path = "../b", version = "0.1.0" }\n',
    );
    const r = runCheck(root, injected(members));
    expect(
      r.violations.some(
        (v) => v.file === "crates/a/Cargo.toml" && v.message.includes('dependency "bc"'),
      ),
    ).toBe(true);
  });

  test("unversioned path dep and external versioned path dep → clean", () => {
    const { root } = fixture();
    const members = withSecondCrate(root);
    writeFileSync(
      join(root, "crates/a/Cargo.toml"),
      '[package]\nname = "a"\nversion = "0.1.0"\n\n[dependencies]\nbc = { path = "../b" }\next = { path = "../ext", version = "1.0.0" }\nserde = "1"\n',
    );
    const r = runCheck(root, injected(members));
    expect(r.violations.filter((v) => v.file === "crates/a/Cargo.toml")).toEqual([]);
  });
});
