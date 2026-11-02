import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { RegistryEntry } from "../lib/registry.ts";
import { applyEntry, applyHelmExtras } from "./bump.ts";

const roots: string[] = [];

function fixture(): string {
  const root = mkdtempSync(join(tmpdir(), "ver-bump-"));
  roots.push(root);
  mkdirSync(join(root, "deploy/helm/x"), { recursive: true });
  mkdirSync(join(root, "crates/a"), { recursive: true });
  writeFileSync(
    join(root, "deploy/helm/x/Chart.yaml"),
    'name: x\nversion: "0.1.0"\nappVersion: "0.4.0"\n',
  );
  writeFileSync(
    join(root, "deploy/helm/x/values.yaml"),
    "api:\n  image:\n    repository: ghcr.io/x/api\n    tag: \"\"\nweb:\n  image:\n    repository: ghcr.io/x/web\n    tag: \"\"\n",
  );
  writeFileSync(
    join(root, "crates/a/Cargo.toml"),
    '[package]\nname = "a"\nversion = "0.4.1"\n',
  );
  return root;
}

const helmEntry = (over: Partial<RegistryEntry> = {}): RegistryEntry => ({
  id: "chart",
  kind: "helm",
  manifest_path: "deploy/helm/x/Chart.yaml",
  tag_prefix: "chart-v",
  changelog_path: "deploy/helm/x/CHANGELOG.md",
  globs: ["deploy/helm/x/**"],
  scopes: ["helm"],
  depends_on: ["a"],
  app_component: "a",
  image_components: { api: "a", web: "a" },
  ...over,
});

const crateEntry = (): RegistryEntry => ({
  id: "a",
  kind: "crate",
  manifest_path: "crates/a/Cargo.toml",
  tag_prefix: "a-v",
  changelog_path: "crates/a/CHANGELOG.md",
  globs: ["crates/a/**"],
  scopes: ["a"],
  depends_on: [],
});

afterEach(() => {
  for (const r of roots.splice(0)) rmSync(r, { recursive: true, force: true });
});

describe("applyEntry — Chart.yaml", () => {
  test("bumps the top-level version scalar", () => {
    const root = fixture();
    const r = applyEntry(root, helmEntry(), "0.1.0", "0.2.0");
    expect("error" in r).toBe(false);
    if ("error" in r) return;
    expect(r.field).toBe("chart.version");
    const text = readFileSync(join(root, "deploy/helm/x/Chart.yaml"), "utf-8");
    expect(text).toContain('version: "0.2.0"');
    expect(text).toContain('appVersion: "0.4.0"');
  });
});

describe("applyHelmExtras", () => {
  test("stamps appVersion + image tags from the app component carrier", () => {
    const root = fixture();
    const byId = new Map([["a", crateEntry()], ["chart", helmEntry()]]);
    const out = applyHelmExtras(root, helmEntry(), byId);
    const errors = out.filter((x) => "error" in x);
    expect(errors).toEqual([]);
    const chart = readFileSync(join(root, "deploy/helm/x/Chart.yaml"), "utf-8");
    expect(chart).toContain('appVersion: "0.4.1"');
    const values = readFileSync(join(root, "deploy/helm/x/values.yaml"), "utf-8");
    expect(values).toContain('tag: "0.4.1"');
    expect(values.match(/tag: "0.4.1"/g)!.length).toBe(2);
  });

  test("idempotent — second run reports no edits", () => {
    const root = fixture();
    const byId = new Map([["a", crateEntry()], ["chart", helmEntry()]]);
    applyHelmExtras(root, helmEntry(), byId);
    const second = applyHelmExtras(root, helmEntry(), byId).filter((x) => !("error" in x));
    expect(second).toEqual([]);
  });

  test("unresolvable app_component → error, no crash", () => {
    const root = fixture();
    const byId = new Map<string, RegistryEntry>();
    const out = applyHelmExtras(root, helmEntry(), byId);
    expect(out.some((x) => "error" in x && x.error.includes("app_component"))).toBe(true);
  });

  test("shared image pins the released carrier's version, not the max", () => {
    const root = fixture();
    // web image shared by carriers a (0.9.0) and b (0.4.1): bumping b must
    // pin 0.4.1 — the tag the release just produced — not max(0.9.0).
    mkdirSync(join(root, "crates/b"), { recursive: true });
    writeFileSync(join(root, "crates/b/Cargo.toml"), '[package]\nname = "b"\nversion = "0.4.1"\n');
    writeFileSync(
      join(root, "crates/a/Cargo.toml"),
      '[package]\nname = "a"\nversion = "0.9.0"\n',
    );
    const entry = helmEntry({ image_components: { web: ["a", "b"] } });
    const byId = new Map([
      ["a", crateEntry()],
      ["b", { ...crateEntry(), id: "b", manifest_path: "crates/b/Cargo.toml" }],
      ["chart", entry],
    ]);
    const out = applyHelmExtras(root, entry, byId, new Map([["b", "0.4.1"]]));
    expect(out.filter((x) => "error" in x)).toEqual([]);
    const values = readFileSync(join(root, "deploy/helm/x/values.yaml"), "utf-8");
    const m = values.match(/^web:[\s\S]*?^    tag: "([^"]*)"/m);
    expect(m?.[1]).toBe("0.4.1");
  });

  test("no carrier in the plan keeps the existing pin", () => {
    const root = fixture();
    writeFileSync(
      join(root, "deploy/helm/x/values.yaml"),
      "web:\n  image:\n    repository: ghcr.io/x/web\n    tag: \"0.4.0\"\n",
    );
    const entry = helmEntry({ image_components: { web: ["a"] } });
    const byId = new Map([["a", crateEntry()], ["chart", entry]]);
    // Chart-only bump: carrier "a" did not release — the pin stays at the
    // last produced tag instead of drifting.
    const out = applyHelmExtras(root, entry, byId, new Map([["chart", "0.2.0"]]));
    expect(out.filter((x) => "error" in x)).toEqual([]);
    expect(out.filter((x) => !("error" in x) && x.field === "chart.image_tag")).toEqual([]);
    const values = readFileSync(join(root, "deploy/helm/x/values.yaml"), "utf-8");
    expect(values).toContain('tag: "0.4.0"');
  });
});
