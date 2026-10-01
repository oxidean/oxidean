/**
 * Shared helpers for change-aware UI coverage gates.
 * Used by browser-coverage-check and route-coverage-check.
 */
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const HARNESS_RE = /\.(?:browser-)?harness\.tsrx$|\.test\./;

export type UiCoverageDiff = {
  /** Paths relative to apps/web/src/ that were added or modified */
  touched: string[];
  /** Subset of touched that are newly added */
  added: string[];
  base: string;
};

function normalizeSrcRel(path: string): string | null {
  const norm = path.split("\\").join("/");
  const prefix = "apps/web/src/";
  if (!norm.startsWith(prefix) || !norm.endsWith(".tsrx")) return null;
  const rel = norm.slice(prefix.length);
  if (HARNESS_RE.test(rel)) return null;
  return rel;
}

function parsePathList(raw: string | undefined): string[] {
  if (!raw?.trim()) return [];
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
    .map((p) => p.replace(/^apps\/web\/src\//, ""))
    .filter((p) => p.endsWith(".tsrx") && !HARNESS_RE.test(p));
}

/**
 * Resolve changed product `.tsrx` under apps/web/src vs a git base ref.
 * Returns null when no base is configured (inventory-only mode).
 *
 * Env overrides (for contract tests):
 * - `UI_COVERAGE_TOUCHED` — modified+added paths (src-relative or apps/web/src/…)
 * - `UI_COVERAGE_ADDED` — subset treated as newly added (defaults to empty)
 */
export function resolveUiCoverageDiff(root: string): UiCoverageDiff | null {
  const touchedOverride = parsePathList(process.env.UI_COVERAGE_TOUCHED);
  if (touchedOverride.length) {
    const addedOverride = parsePathList(process.env.UI_COVERAGE_ADDED);
    const touched = [...new Set(touchedOverride)].sort();
    const added = [...new Set(addedOverride.filter((p) => touched.includes(p)))].sort();
    return { touched, added, base: "env:UI_COVERAGE_TOUCHED" };
  }

  const requested =
    process.env.UI_COVERAGE_BASE?.trim() ||
    process.env.BROWSER_COVERAGE_BASE?.trim() ||
    "";
  if (!requested) return null;

  const git = (args: string[]) =>
    spawnSync("git", ["-C", root, ...args], {
      encoding: "utf8",
      maxBuffer: 8 * 1024 * 1024,
    });

  /** Prefer requested base; fall back when force-pushes orphan github.event.before. */
  const candidates = [requested, "origin/main", "main"].filter(
    (v, i, arr) => v && arr.indexOf(v) === i,
  );

  let base = "";
  for (const cand of candidates) {
    const rev = git(["rev-parse", "--verify", cand]);
    if (rev.status !== 0) continue;
    const mb = git(["merge-base", cand, "HEAD"]);
    if (mb.status !== 0) continue;
    base = cand;
    if (cand !== requested) {
      console.warn(
        `ui-coverage-diff: UI_COVERAGE_BASE=${requested} unusable; falling back to ${cand}`,
      );
    }
    break;
  }

  if (!base) {
    console.warn(
      `ui-coverage-diff: no usable UI_COVERAGE_BASE (tried ${candidates.join(", ")}); inventory-only`,
    );
    return null;
  }

  const nameOnly = (diffFilter: string) => {
    const r = git([
      "diff",
      "--name-only",
      `--diff-filter=${diffFilter}`,
      `${base}...HEAD`,
      "--",
      "apps/web/src",
    ]);
    if (r.status !== 0) {
      console.error(
        `ui-coverage-diff: FAIL: git diff failed: ${r.stderr || r.stdout}`,
      );
      process.exit(1);
    }
    return (r.stdout || "")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean)
      .map(normalizeSrcRel)
      .filter((x): x is string => Boolean(x));
  };

  const touched = [...new Set(nameOnly("ACMR"))].sort();
  const added = [...new Set(nameOnly("A"))].sort();

  // Drop paths that no longer exist (renames / deletes already filtered by ACMR).
  const existing = touched.filter((rel) =>
    existsSync(join(resolve(root, "apps/web/src"), rel)),
  );

  return { touched: existing, added, base };
}

export function formatTouchedList(paths: string[]): string {
  return paths.map((p) => `  - ${p}`).join("\n");
}
