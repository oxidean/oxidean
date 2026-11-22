import type { PullCommentPublic } from "@oxidean/api-client";

/**
 * COL-08 — client mirror of the server's `suggestion_blocks` fence parser
 * (`crates/oxidean-api-pulls/src/suggestions.rs`). Splits a review-comment
 * body into markdown and ````suggestion` segments so the diff surface can
 * render suggested diffs and wire apply controls; non-suggestion surfaces
 * get the labeled block via `renderGfm`'s rehype transform.
 */

export type SuggestionSegment =
  | { kind: "md"; text: string }
  | { kind: "suggestion"; text: string; index: number };

type Fence = { ch: string; len: number };

/** Leading fence run on a trimmed line: backtick/tilde runs of length >=3. */
function fenceMarker(trimmed: string): Fence | null {
  const ch = trimmed[0];
  if (ch !== "`" && ch !== "~") return null;
  let len = 0;
  while (len < trimmed.length && trimmed[len] === ch) len++;
  return len >= 3 ? { ch, len } : null;
}

/** Closer line: up to three leading spaces, then a matching fence run. */
function isFenceClose(line: string, fence: Fence): boolean {
  let indent = 0;
  while (line[indent] === " ") indent++;
  if (indent > 3) return false;
  const t = line.slice(indent).replace(/ +$/, "");
  if (!t) return false;
  for (const c of t) {
    if (c !== fence.ch) return false;
  }
  return t.length >= fence.len;
}

/** GFM content de-indent: drop up to `indent` leading spaces. */
function stripIndent(line: string, indent: number): string {
  let i = 0;
  while (i < line.length && i < indent && line[i] === " ") i++;
  return line.slice(i);
}

/**
 * Ordered body segments. `index` is the suggestion's position among
 * suggestion fences — it is the `suggestionIndex` the apply RPC takes.
 * GFM rules: opener <=3-space indent; closer is a same-char run >= opener
 * length; non-suggestion fences are skipped whole so a nested
 * ````suggestion` inside code never counts; EOF closes an open fence.
 */
export function commentBodySegments(body: string): SuggestionSegment[] {
  const lines = body.split(/\r?\n/);
  const out: SuggestionSegment[] = [];
  let mdStart = 0;
  let index = 0;
  let i = 0;
  const pushMd = (end: number) => {
    const text = lines.slice(mdStart, end).join("\n");
    mdStart = end;
    if (text.trim()) out.push({ kind: "md", text });
  };
  while (i < lines.length) {
    const line = lines[i];
    let indent = 0;
    while (line[indent] === " ") indent++;
    const stripped = line.slice(indent);
    const fence = indent <= 3 ? fenceMarker(stripped) : null;
    if (!fence) {
      i++;
      continue;
    }
    if (stripped.slice(fence.len).trim() !== "suggestion") {
      i++;
      while (i < lines.length && !isFenceClose(lines[i], fence)) i++;
      i++; // closer (or EOF)
      continue;
    }
    i++;
    const buf: string[] = [];
    while (i < lines.length && !isFenceClose(lines[i], fence)) {
      buf.push(stripIndent(lines[i], indent));
      i++;
    }
    i++; // closer (or EOF)
    pushMd(i - (buf.length + 1) - 1); // md before the opener line
    out.push({ kind: "suggestion", text: buf.join("\n"), index: index++ });
    mdStart = i;
  }
  pushMd(lines.length);
  return out;
}

/** Number of ````suggestion` fences in a comment body. */
export function suggestionBlockCount(body: string): number {
  return commentBodySegments(body).reduce((n, s) => n + (s.kind === "suggestion" ? 1 : 0), 0);
}

/**
 * Server-parity check for the apply affordance: a submitted (not pending),
 * not-outdated comment anchored on a RIGHT-side line/range carrying at
 * least one suggestion fence. The server re-validates everything.
 */
export function suggestionAppliable(c: PullCommentPublic): boolean {
  if (c.pending || c.outdated || c.line == null || !c.path) return false;
  if (c.side !== "RIGHT") return false;
  return suggestionBlockCount(c.body) > 0;
}
