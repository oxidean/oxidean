import { describe, expect, test } from "bun:test";
import { loadRegistry, parseRegistry } from "./registry.ts";
import { globsMayOverlap } from "./globs.ts";

const VALID = `version = 1

[baseline]
commit = "pending"

[grammar]
allowed_types = ["feat", "fix"]
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
depends_on = ["a"]
`;

function withB(mutate: (text: string) => string): string {
  return mutate(VALID);
}

describe("parseRegistry — happy path", () => {
  test("valid registry parses to typed config", () => {
    const r = parseRegistry(VALID);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.config.baseline_commit).toBe("pending");
    expect(r.config.allowed_types).toEqual(["feat", "fix"]);
    expect(r.config.entries.map((e) => e.id)).toEqual(["a", "b"]);
    expect(r.config.entries[1].depends_on).toEqual(["a"]);
  });
});

describe("parseRegistry — strict schema (BR1.3)", () => {
  test("unknown top-level key → violation", () => {
    const r = parseRegistry(withB((t) => t + '\nsurprise = 1\n'));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('"surprise"'))).toBe(true);
  });

  test("unknown component key → violation", () => {
    const r = parseRegistry(
      withB((t) => t.replace('tag_prefix = "a-v"', 'tag_prefix = "a-v"\nbogus = 1')),
    );
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('"bogus"'))).toBe(true);
  });
});

describe("parseRegistry — kind/carrier (BR1.1)", () => {
  test("crate without manifest_path → violation", () => {
    const r = parseRegistry(
      withB((t) => t.replace('manifest_path = "crates/a/Cargo.toml"\n', "")),
    );
    expect(r.ok).toBe(false);
  });

  test("contract with manifest_path → violation", () => {
    const r = parseRegistry(
      withB(
        (t) =>
          t +
          `\n[[component]]\nid = "w"\nkind = "contract"\nmanifest_path = "x/Cargo.toml"\nconstant_path = "f.rs#V"\ntag_prefix = "w-v"\nchangelog_path = "c.md"\nglobs = []\nscopes = ["w"]\ndepends_on = []\n`,
      ),
    );
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes("must not carry manifest_path"))).toBe(true);
  });

  test("contract without constant_path → violation", () => {
    const r = parseRegistry(
      withB(
        (t) =>
          t +
          `\n[[component]]\nid = "w"\nkind = "contract"\ntag_prefix = "w-v"\nchangelog_path = "c.md"\nglobs = []\nscopes = ["w"]\ndepends_on = []\n`,
      ),
    );
    expect(r.ok).toBe(false);
  });
});

describe("parseRegistry — uniqueness (BR1.7)", () => {
  test("duplicate scope alias → violation", () => {
    const r = parseRegistry(withB((t) => t.replace('scopes = ["b"]', 'scopes = ["a"]')));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('scope alias "a"'))).toBe(true);
  });

  test("duplicate tag_prefix → violation", () => {
    const r = parseRegistry(withB((t) => t.replace('tag_prefix = "b-v"', 'tag_prefix = "a-v"')));
    expect(r.ok).toBe(false);
  });

  test("reserved scope claimed → violation", () => {
    const r = parseRegistry(withB((t) => t.replace('scopes = ["b"]', 'scopes = ["release"]')));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes("reserved"))).toBe(true);
  });
});

describe("globsMayOverlap (BR1.7)", () => {
  test("parent ** vs child literal path → overlap", () => {
    expect(globsMayOverlap("crates/**", "crates/oxidean-api/**")).toBe(true);
  });
  test("sibling dirs → disjoint", () => {
    expect(globsMayOverlap("crates/a/**", "crates/b/**")).toBe(false);
    expect(globsMayOverlap("apps/web/**", "packages/api-client/**")).toBe(false);
  });
  test("segment wildcard vs literal may collide → overlap", () => {
    expect(globsMayOverlap("crates/*/Cargo.toml", "crates/x/**")).toBe(true);
  });
  test("distinct literal tails → disjoint", () => {
    expect(globsMayOverlap("a/x.rs", "a/y.rs")).toBe(false);
  });
  test("registry parse flags overlapping globs across entries", () => {
    const r = parseRegistry(withB((t) => t.replace('globs = ["apps/b/**"]', 'globs = ["crates/**"]')));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes("overlap"))).toBe(true);
  });
});

describe("parseRegistry — depends_on DAG (BR1.4)", () => {
  test("unknown dep id → violation", () => {
    const r = parseRegistry(withB((t) => t.replace('depends_on = ["a"]', 'depends_on = ["ghost"]')));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('unknown component "ghost"'))).toBe(true);
  });

  test("self-edge → violation", () => {
    const r = parseRegistry(withB((t) => t.replace('depends_on = ["a"]', 'depends_on = ["b"]')));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes("itself"))).toBe(true);
  });

  test("cycle → violation naming the cycle", () => {
    const r = parseRegistry(
      withB((t) => t.replace('depends_on = []', 'depends_on = ["b"]').replace('depends_on = ["a"]\n\n[[component]]\nid = "b"', 'depends_on = ["a"]\n\n[[component]]\nid = "b"')),
    );
    // a now depends on b, b depends on a → cycle
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes("cycle"))).toBe(true);
  });
});

describe("parseRegistry — baseline (BR1.6)", () => {
  test("missing baseline.commit → violation", () => {
    const r = parseRegistry(withB((t) => t.replace('commit = "pending"\n', "")));
    expect(r.ok).toBe(false);
  });
  test("non-sha40 commit → violation", () => {
    const r = parseRegistry(withB((t) => t.replace('commit = "pending"', 'commit = "abc"')));
    expect(r.ok).toBe(false);
  });
  test("sha40 commit → ok", () => {
    const r = parseRegistry(withB((t) => t.replace('commit = "pending"', `commit = "${"a".repeat(40)}"`)));
    expect(r.ok).toBe(true);
  });
});

const HELM = `\n[[component]]\nid = "chart"\nkind = "helm"\nmanifest_path = "deploy/helm/x/Chart.yaml"\ntag_prefix = "chart-v"\nchangelog_path = "deploy/helm/x/CHANGELOG.md"\nglobs = ["deploy/helm/x/**"]\nscopes = ["helm"]\ndepends_on = ["a", "b"]\napp_component = "a"\nimage_components = { b = "b" }\n`;

describe("parseRegistry — kind=helm linkages", () => {
  test("valid helm entry parses typed fields", () => {
    const r = parseRegistry(VALID + HELM);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const chart = r.config.entries.find((e) => e.id === "chart")!;
    expect(chart.kind).toBe("helm");
    expect(chart.app_component).toBe("a");
    expect(chart.image_components).toEqual({ b: "b" });
  });

  test("helm without app_component → violation", () => {
    const r = parseRegistry(VALID + HELM.replace('app_component = "a"\n', ""));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes("requires app_component"))).toBe(true);
  });

  test("app_component not in depends_on → violation", () => {
    const r = parseRegistry(VALID + HELM.replace('depends_on = ["a", "b"]', 'depends_on = ["b"]'));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('app_component "a" must appear in depends_on'))).toBe(true);
  });

  test("image_components referencing unknown id → violation", () => {
    const r = parseRegistry(VALID + HELM.replace('image_components = { b = "b" }', 'image_components = { b = "ghost" }'));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('unknown component "ghost"'))).toBe(true);
  });

  test("image_components not in depends_on → violation", () => {
    const r = parseRegistry(VALID + HELM.replace('depends_on = ["a", "b"]', 'depends_on = ["a"]'));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('image_components.b = "b" must appear in depends_on'))).toBe(true);
  });

  test("image_components array value parses (shared-image multi-source)", () => {
    const r = parseRegistry(VALID + HELM.replace('image_components = { b = "b" }', 'image_components = { b = ["a", "b"] }'));
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const chart = r.config.entries.find((e) => e.id === "chart")!;
    expect(chart.image_components).toEqual({ b: ["a", "b"] });
  });

  test("image_components array with unknown id → violation", () => {
    const r = parseRegistry(VALID + HELM.replace('image_components = { b = "b" }', 'image_components = { b = ["a", "ghost"] }'));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('unknown component "ghost"'))).toBe(true);
  });

  test("image_components empty array → violation", () => {
    const r = parseRegistry(VALID + HELM.replace('image_components = { b = "b" }', 'image_components = { b = [] }'));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes('must be a component id or non-empty id list'))).toBe(true);
  });

  test("crate carrying app_component → violation", () => {
    const r = parseRegistry(VALID.replace('depends_on = []\n', 'depends_on = []\napp_component = "b"\n'));
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations.some((v) => v.message.includes("must not carry app_component"))).toBe(true);
  });
});

describe("loadRegistry — file states", () => {
  test("missing file → registry not found violation", () => {
    const r = loadRegistry("/nonexistent-dir-12345");
    expect(r.ok).toBe(false);
    if (r.ok) return;
    expect(r.violations[0].message).toContain("registry not found");
  });
});
