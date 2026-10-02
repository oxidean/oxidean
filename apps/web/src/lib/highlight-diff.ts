import type { ThemedToken } from "shiki";
import { getHighlighter, languageIdForPath, THEMES, type HighlightTheme } from "@/lib/highlight";
import type { DiffLine, DiffLineKind } from "@/lib/parse-unified-diff";

/** Skip Shiki when reconstructed sides exceed this (keep first paint snappy). */
export const DIFF_HIGHLIGHT_SOFT_MAX_CHARS = 200_000;

export type HighlightedDiffLine = {
  kind: DiffLineKind;
  /** Original unified-diff line (including leading +/- / space). */
  text: string;
  /** Leading marker for code lines (`+` / `-` / ` `); empty for meta/hunk/note. */
  prefix: string;
  /** Escaped / syntax-highlighted HTML for the code (or full line for meta). */
  contentHtml: string;
};

export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Split a unified-diff code line into marker + payload. */
export function splitDiffLinePrefix(
  kind: DiffLineKind,
  text: string,
): { prefix: string; content: string } {
  if (kind === "add" || kind === "del" || kind === "ctx") {
    if (text === "") return { prefix: "", content: "" };
    return { prefix: text.slice(0, 1), content: text.slice(1) };
  }
  return { prefix: "", content: text };
}

function isCodeLine(kind: DiffLineKind): boolean {
  return kind === "add" || kind === "del" || kind === "ctx";
}

/** True when row HTML already matches (skip setState to avoid SSR→hydrate flicker). */
export function highlightedDiffRowsEqual(
  a: HighlightedDiffLine[],
  b: HighlightedDiffLine[],
): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    const x = a[i]!;
    const y = b[i]!;
    if (
      x.kind !== y.kind ||
      x.prefix !== y.prefix ||
      x.contentHtml !== y.contentHtml ||
      x.text !== y.text
    ) {
      return false;
    }
  }
  return true;
}

/** Plain (escaped) rows — sync first paint before / instead of Shiki. */
export function plainHighlightedDiffLines(lines: DiffLine[]): HighlightedDiffLine[] {
  return lines.map((line) => {
    const { prefix, content } = splitDiffLinePrefix(line.kind, line.text);
    if (isCodeLine(line.kind)) {
      return {
        kind: line.kind,
        text: line.text,
        prefix,
        contentHtml: escapeHtml(content),
      };
    }
    return {
      kind: line.kind,
      text: line.text,
      prefix: "",
      contentHtml: escapeHtml(line.text),
    };
  });
}

/** Convert Shiki tokens for one line into inline spans (no pre/code wrapper). */
export function tokensToInlineHtml(tokens: ThemedToken[]): string {
  return tokens
    .map((t) => {
      const body = escapeHtml(t.content);
      if (!t.color) return body;
      const parts = [`color:${t.color}`];
      // FontStyle bitflags from vscode-textmate / Shiki.
      if (t.fontStyle & 1) parts.push("font-style:italic");
      if (t.fontStyle & 2) parts.push("font-weight:bold");
      if (t.fontStyle & 4) parts.push("text-decoration:underline");
      return `<span style="${parts.join(";")}">${body}</span>`;
    })
    .join("");
}

type SideBuild = {
  oldParts: string[];
  newParts: string[];
  /** Per diff line: index into oldParts / newParts when present. */
  map: Array<{ oldIdx?: number; newIdx?: number }>;
};

function buildSides(lines: DiffLine[]): SideBuild {
  const oldParts: string[] = [];
  const newParts: string[] = [];
  const map: SideBuild["map"] = lines.map(() => ({}));

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (!isCodeLine(line.kind)) continue;
    const { content } = splitDiffLinePrefix(line.kind, line.text);
    if (line.kind === "del" || line.kind === "ctx") {
      map[i]!.oldIdx = oldParts.length;
      oldParts.push(content);
    }
    if (line.kind === "add" || line.kind === "ctx") {
      map[i]!.newIdx = newParts.length;
      newParts.push(content);
    }
  }

  return { oldParts, newParts, map };
}

async function highlightSideLines(
  parts: string[],
  lang: string,
  theme: HighlightTheme,
): Promise<string[]> {
  if (parts.length === 0) return [];
  const highlighter = await getHighlighter();
  const loaded = highlighter.getLoadedLanguages();
  const resolved = loaded.includes(lang) ? lang : "plaintext";
  const { tokens } = highlighter.codeToTokens(parts.join("\n"), {
    lang: resolved,
    theme,
  });
  // codeToTokens yields one token row per input line (including empty).
  return parts.map((_, i) => tokensToInlineHtml(tokens[i] ?? []));
}

/**
 * Syntax-highlight add/del/ctx payloads by file language.
 * Reconstructs old (ctx+del) and new (ctx+add) sides so grammars see multi-line context,
 * then maps tokens back onto unified rows. Meta/hunk/note stay escaped plain text.
 *
 * Skips Shiki when reconstructed content exceeds {@link DIFF_HIGHLIGHT_SOFT_MAX_CHARS}.
 */
export async function highlightDiffLines(
  lines: DiffLine[],
  options: { path: string; theme?: HighlightTheme },
): Promise<HighlightedDiffLine[]> {
  const plain = plainHighlightedDiffLines(lines);
  if (lines.length === 0) return plain;

  const { oldParts, newParts, map } = buildSides(lines);
  const totalChars =
    oldParts.reduce((n, s) => n + s.length + 1, 0) + newParts.reduce((n, s) => n + s.length + 1, 0);
  if (totalChars > DIFF_HIGHLIGHT_SOFT_MAX_CHARS) {
    return plain;
  }

  const lang = languageIdForPath(options.path);
  if (lang === "plaintext") {
    return plain;
  }

  const theme = options.theme ?? THEMES.dark;
  const [oldHtml, newHtml] = await Promise.all([
    highlightSideLines(oldParts, lang, theme),
    highlightSideLines(newParts, lang, theme),
  ]);

  return lines.map((line, i) => {
    const base = plain[i]!;
    if (!isCodeLine(line.kind)) return base;

    const side = map[i]!;
    let contentHtml = base.contentHtml;
    if (line.kind === "del" && side.oldIdx !== undefined) {
      contentHtml = oldHtml[side.oldIdx] ?? contentHtml;
    } else if (line.kind === "add" && side.newIdx !== undefined) {
      contentHtml = newHtml[side.newIdx] ?? contentHtml;
    } else if (line.kind === "ctx" && side.newIdx !== undefined) {
      // Prefer post-image highlighting for context.
      contentHtml = newHtml[side.newIdx] ?? contentHtml;
    }

    return { ...base, contentHtml };
  });
}
