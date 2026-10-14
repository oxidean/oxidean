/**
 * SemVer math for the bump engine (BR3.4–BR3.6, FR3.1, FR8.3).
 * Pure functions — no I/O; callers supply prior-tag counts.
 */
import { isSemver } from "./registry.ts";

export type BumpLevel = "none" | "patch" | "minor" | "major";

const RANK: Record<BumpLevel, number> = { none: 0, patch: 1, minor: 2, major: 3 };

export function maxBump(a: BumpLevel, b: BumpLevel): BumpLevel {
  return RANK[a] >= RANK[b] ? a : b;
}

export interface SemverParts {
  major: number;
  minor: number;
  patch: number;
  /** Prerelease suffix without the leading `-`, if any. */
  pre?: string;
}

export function parseSemver(v: string): SemverParts | null {
  if (!isSemver(v)) return null;
  const [core, pre] = v.split("-", 2);
  const [major, minor, patch] = core.split(".").map(Number);
  return { major, minor, patch, ...(pre ? { pre } : {}) };
}

/**
 * FR3.1 — 0.x-aware table: `breaking` escalates to minor (<1.0.0) or major
 * (>=1.0.0) regardless of type; otherwise feat→minor, fix/perf/revert→patch,
 * housekeeping (chore/docs/style/refactor/test/build/ci)→none.
 * A `type` of undefined (grammar-invalid commit) counts as patch — a commit
 * that touched the component's files is real evidence; fail toward releasing.
 */
export function bumpForCommit(
  type: string | undefined,
  breaking: boolean,
  fromVersion: string,
): BumpLevel {
  if (breaking) {
    const v = parseSemver(fromVersion);
    return v && v.major === 0 ? "minor" : "major";
  }
  switch (type) {
    case "feat":
      return "minor";
    case "fix":
    case "perf":
    case "revert":
      return "patch";
    case undefined:
      return "patch";
    default:
      return "none";
  }
}

/** Apply a bump level to a semver base, dropping any existing prerelease. */
export function applyBump(fromVersion: string, level: BumpLevel): string {
  const v = parseSemver(fromVersion);
  if (v === null) throw new Error(`cannot bump non-semver "${fromVersion}"`);
  const base = `${v.major}.${v.minor}.${v.patch}`;
  switch (level) {
    case "none":
      return base;
    case "patch":
      return `${v.major}.${v.minor}.${v.patch + 1}`;
    case "minor":
      return `${v.major}.${v.minor + 1}.0`;
    case "major":
      return `${v.major + 1}.0.0`;
  }
}

export type Channel = "stable" | "alpha" | "beta" | "nightly";

export const CHANNELS: readonly Channel[] = ["stable", "alpha", "beta", "nightly"];

function today(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getUTCFullYear()}${p(d.getUTCMonth() + 1)}${p(d.getUTCDate())}`;
}

/**
 * BR3.6 — channel suffix. `stable` returns the bare base; `alpha`/`beta`
 * append `-<channel>.N`; `nightly` appends `-nightly.YYYYMMDDN`.
 * `priorCount` is the number of existing tags of this component on this
 * channel for the same base — N = priorCount + 1 (per-base restart at 1).
 */
export function withChannel(base: string, channel: Channel, priorCount: number): string {
  if (channel === "stable") return base;
  const n = priorCount + 1;
  if (channel === "nightly") return `${base}-nightly.${today()}${n}`;
  return `${base}-${channel}.${n}`;
}

/** Extract the prerelease tag counter N, e.g. `alpha.3` → 3, `nightly.202610142` → NaN-free suffix. */
export function prereleaseOf(v: string): string | null {
  const i = v.indexOf("-");
  return i === -1 ? null : v.slice(i + 1);
}

/** Compare two semver strings; returns -1/0/1. Prerelease < its base. */
export function compareSemver(a: string, b: string): number {
  const pa = parseSemver(a);
  const pb = parseSemver(b);
  if (pa === null || pb === null) return a.localeCompare(b);
  for (const k of ["major", "minor", "patch"] as const) {
    if (pa[k] !== pb[k]) return pa[k] < pb[k] ? -1 : 1;
  }
  if (pa.pre === pb.pre) return 0;
  if (pa.pre === undefined) return 1;
  if (pb.pre === undefined) return -1;
  const as = pa.pre.split(".");
  const bs = pb.pre.split(".");
  for (let i = 0; i < Math.max(as.length, bs.length); i++) {
    const x = as[i];
    const y = bs[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const nx = Number(x);
    const ny = Number(y);
    const numX = Number.isInteger(nx) && x === String(nx);
    const numY = Number.isInteger(ny) && y === String(ny);
    if (numX && numY) {
      if (nx !== ny) return nx < ny ? -1 : 1;
    } else if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return 0;
}
