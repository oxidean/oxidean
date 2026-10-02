import { createHighlighter, type Highlighter } from "shiki";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import { readThemePreference, resolveTheme } from "@/lib/theme";
import tsrxGrammar from "./grammars/tsrx.tmLanguage.json";
import rippleGrammar from "./grammars/ripple.tmLanguage.json";

/** Cool-biased GitHub themes (D-19 / plan: github-light + github-dark). */
const THEMES = ["github-light", "github-dark"] as const;

export type HighlightTheme = (typeof THEMES)[number];

/**
 * Resolve github-light / github-dark for client highlighting.
 * Prefer `html.dark` (set by the FOUC boot script) so we match SSR + first paint
 * instead of re-deriving from localStorage/matchMedia and causing a flicker.
 */
export function clientHighlightTheme(): HighlightTheme {
  if (typeof document !== "undefined") {
    return document.documentElement.classList.contains("dark") ? "github-dark" : "github-light";
  }
  return resolveTheme(readThemePreference()) === "dark" ? "github-dark" : "github-light";
}

const GITHUB_CLASS_LANGS = [
  "typescript",
  "tsx",
  "javascript",
  "jsx",
  "json",
  "markdown",
  "html",
  "css",
  "scss",
  "python",
  "rust",
  "go",
  "bash",
  "shell",
  "yaml",
  "toml",
  "sql",
  "dockerfile",
  "diff",
  "plaintext",
] as const;

/** Official TSRX grammar + Shiki embedded langs (oxc-tsrx / tsrx-org pattern). */
const tsrxLang = {
  ...tsrxGrammar,
  name: "tsrx",
  scopeName: "source.tsrx",
  embeddedLangs: ["jsx", "tsx", "css"] as const,
};

/** Full Ripple grammar + embedded langs. */
const rippleLang = {
  ...rippleGrammar,
  name: "ripple",
  scopeName: "source.ripple",
  embeddedLangs: ["jsx", "tsx", "css"] as const,
};

let highlighterPromise: Promise<Highlighter> | null = null;

/**
 * Singleton Shiki highlighter with common langs + in-repo tsrx/ripple grammars (D-19).
 * Custom langs are full TextMate grammars — not TypeScript/JavaScript aliases.
 * JS regex engine (forgiving) matches official TSRX demo for large TM grammars.
 */
export async function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [...THEMES],
      langs: [...GITHUB_CLASS_LANGS, tsrxLang, rippleLang],
      engine: createJavaScriptRegexEngine({ forgiving: true }),
    });
  }
  return highlighterPromise;
}

/** Map a repo path to a Shiki language id (including .tsrx / .ripple). */
export function languageIdForPath(filePath: string): string {
  const base = filePath.split(/[/\\]/).pop() ?? filePath;
  const lower = base.toLowerCase();
  const dot = lower.lastIndexOf(".");
  const ext = dot >= 0 ? lower.slice(dot) : "";

  switch (ext) {
    case ".tsrx":
      return "tsrx";
    case ".ripple":
      return "ripple";
    case ".ts":
      return "typescript";
    case ".tsx":
      return "tsx";
    case ".js":
    case ".mjs":
    case ".cjs":
      return "javascript";
    case ".jsx":
      return "jsx";
    case ".md":
    case ".markdown":
      return "markdown";
    case ".json":
      return "json";
    case ".html":
    case ".htm":
      return "html";
    case ".css":
      return "css";
    case ".scss":
      return "scss";
    case ".py":
      return "python";
    case ".rs":
      return "rust";
    case ".go":
      return "go";
    case ".sh":
    case ".bash":
      return "bash";
    case ".yml":
    case ".yaml":
      return "yaml";
    case ".toml":
      return "toml";
    case ".sql":
      return "sql";
    case ".dockerfile":
      return "dockerfile";
    case ".diff":
    case ".patch":
      return "diff";
    default:
      if (lower === "dockerfile") return "dockerfile";
      return "plaintext";
  }
}

export async function highlightCode(
  code: string,
  options: { lang: string; theme?: HighlightTheme },
): Promise<string> {
  const highlighter = await getHighlighter();
  const theme = options.theme ?? "github-dark";
  const loaded = highlighter.getLoadedLanguages();
  const lang = loaded.includes(options.lang) ? options.lang : "plaintext";
  // Trailing newline would render as an empty last line without a line number.
  const display = stripTrailingNewline(code);
  const html = highlighter.codeToHtml(display, { lang, theme });
  // Annotate language id for callers/tests — Shiki HTML may omit the lang name.
  return html.replace(/<pre(\s)/, `<pre data-language="${lang}"$1`);
}

/** Drop a single trailing newline so line numbers match highlighted rows. */
export function stripTrailingNewline(code: string): string {
  return code.endsWith("\n") ? code.slice(0, -1) : code;
}

/** Line count for blob chrome — ignores a single trailing newline (POSIX text files). */
export function countCodeLines(code: string): number {
  if (!code) return 0;
  const parts = code.split("\n");
  if (parts.length > 0 && parts[parts.length - 1] === "") {
    return parts.length - 1;
  }
  return parts.length;
}
