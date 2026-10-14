/**
 * Glob helpers for change attribution and the registry's glob-uniqueness
 * rule (BR1.7). Matching uses Bun.Glob; overlap detection is a conservative
 * segment-wise "may a path match both patterns" relation — it never reports
 * disjoint patterns as overlapping, but may conservatively flag patterns that
 * cannot literally produce the same string. Over-flagging is deliberate: a
 * false positive costs a registry edit, a false negative splits attribution.
 */

/** A segment counts as wildcarded when it is `**` or contains * ? [ { */
function isWildcard(seg: string): boolean {
  return seg === "**" || /[*?[{]/.test(seg);
}

function segments(glob: string): string[] {
  return glob.split("/").filter((s) => s.length > 0);
}

/**
 * Conservative overlap test: returns true when the two patterns cannot be
 * proven disjoint. Walks segments pairwise; a position where both segments
 * are distinct literals proves disjointness (all later positions irrelevant).
 * `**` anywhere treated as "can absorb anything at or beyond this point".
 */
export function globsMayOverlap(a: string, b: string): boolean {
  const sa = segments(a);
  const sb = segments(b);
  const n = Math.max(sa.length, sb.length);
  for (let i = 0; i < n; i++) {
    const x = sa[i];
    const y = sb[i];
    if (x === "**" || y === "**") return true;
    if (x === undefined) {
      // a exhausted; b continues. Only overlappable if every remaining
      // b segment is `**` (b can collapse to a's length) — handled above,
      // so a path matching b is strictly deeper than any a-match unless a
      // ended in `**` (also handled above).
      return sa[sa.length - 1] === "**";
    }
    if (y === undefined) {
      return sb[sb.length - 1] === "**";
    }
    if (!isWildcard(x) && !isWildcard(y) && x !== y) return false;
    // equal literals, or either side wildcard → may match; continue
  }
  return true;
}

/** Does `path` match any of `globs`? (Bun.Glob semantics.) */
export function matchesAny(path: string, globs: string[]): boolean {
  return globs.some((g) => new Bun.Glob(g).match(path));
}
