import { LANGUAGES } from "@oxidean/api-client";
import { createHighlighter, type Highlighter, type ThemeRegistration } from "shiki";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import { readThemePreference, resolveTheme } from "@/lib/theme";
import tsrxGrammar from "./grammars/tsrx.tmLanguage.json";
import rippleGrammar from "./grammars/ripple.tmLanguage.json";

/**
 * Branded Oxidean syntax themes (issue #59). Token colors follow the palette
 * in the issue: cool GitHub-adjacent neutrals, red/orange keywords,
 * blue/green strings, teal constants, green functions, purple types.
 */
const oxideanLightTheme: ThemeRegistration = {
  name: "oxidean-light",
  displayName: "Oxidean Light",
  type: "light",
  fg: "#1f2328",
  bg: "#ffffff",
  colors: {
    "editor.foreground": "#1f2328",
    "editor.background": "#ffffff",
  },
  settings: [
    {
      scope: ["comment", "punctuation.definition.comment"],
      settings: { foreground: "#6e7781", fontStyle: "italic" },
    },
    {
      scope: [
        "keyword",
        "keyword.control",
        "storage",
        "storage.type",
        "keyword.operator.new",
        "keyword.operator.expression",
      ],
      settings: { foreground: "#cf222e" },
    },
    {
      scope: ["keyword.operator", "keyword.operator.assignment", "punctuation.accessor"],
      settings: { foreground: "#cf222e" },
    },
    {
      scope: ["string", "string.quoted", "string.template", "punctuation.definition.string"],
      settings: { foreground: "#0a3069" },
    },
    {
      scope: [
        "constant",
        "constant.numeric",
        "constant.language",
        "support.constant",
        "variable.other.constant",
      ],
      settings: { foreground: "#0a7ea4" },
    },
    {
      scope: ["entity.name.function", "support.function", "meta.function-call"],
      settings: { foreground: "#116329" },
    },
    {
      scope: [
        "entity.name.type",
        "entity.name.class",
        "support.type",
        "support.class",
        "entity.name.tag",
        "storage.type.class",
      ],
      settings: { foreground: "#8250df" },
    },
    {
      scope: ["variable", "variable.other", "entity.name.variable"],
      settings: { foreground: "#1f2328" },
    },
    {
      scope: ["variable.language", "variable.parameter", "meta.parameters"],
      settings: { foreground: "#953800" },
    },
    {
      scope: [
        "entity.name.namespace",
        "entity.name.module",
        "meta.import",
        "keyword.import",
        "keyword.export",
      ],
      settings: { foreground: "#8250df" },
    },
    { scope: ["markup.heading"], settings: { foreground: "#0550ae", fontStyle: "bold" } },
    { scope: ["markup.bold"], settings: { fontStyle: "bold" } },
    { scope: ["markup.italic"], settings: { fontStyle: "italic" } },
    { scope: ["markup.inserted"], settings: { foreground: "#116329" } },
    { scope: ["markup.deleted"], settings: { foreground: "#82071e" } },
    { scope: ["markup.changed"], settings: { foreground: "#953800" } },
    { scope: ["invalid", "invalid.illegal"], settings: { foreground: "#82071e" } },
    { scope: ["punctuation", "meta.brace", "meta.delimiter"], settings: { foreground: "#1f2328" } },
    {
      scope: ["entity.other.attribute-name", "meta.attribute"],
      settings: { foreground: "#116329" },
    },
    {
      scope: ["support.type.property-name", "variable.other.property", "meta.object-literal.key"],
      settings: { foreground: "#0550ae" },
    },
    {
      scope: ["markup.raw", "markup.inline.raw", "string.regexp"],
      settings: { foreground: "#116329" },
    },
    {
      scope: ["markup.link", "markup.underline.link", "string.other.link"],
      settings: { foreground: "#0969da" },
    },
  ],
};

const oxideanDarkTheme: ThemeRegistration = {
  name: "oxidean-dark",
  displayName: "Oxidean Dark",
  type: "dark",
  fg: "#e6edf3",
  bg: "#0d1117",
  colors: {
    "editor.foreground": "#e6edf3",
    "editor.background": "#0d1117",
  },
  settings: [
    {
      scope: ["comment", "punctuation.definition.comment"],
      settings: { foreground: "#8b949e", fontStyle: "italic" },
    },
    {
      scope: [
        "keyword",
        "keyword.control",
        "storage",
        "storage.type",
        "keyword.operator.new",
        "keyword.operator.expression",
      ],
      settings: { foreground: "#ff7b72" },
    },
    {
      scope: ["keyword.operator", "keyword.operator.assignment", "punctuation.accessor"],
      settings: { foreground: "#ff7b72" },
    },
    {
      scope: ["string", "string.quoted", "string.template", "punctuation.definition.string"],
      settings: { foreground: "#a5d6ff" },
    },
    {
      scope: [
        "constant",
        "constant.numeric",
        "constant.language",
        "support.constant",
        "variable.other.constant",
      ],
      settings: { foreground: "#39c5cf" },
    },
    {
      scope: ["entity.name.function", "support.function", "meta.function-call"],
      settings: { foreground: "#7ee787" },
    },
    {
      scope: [
        "entity.name.type",
        "entity.name.class",
        "support.type",
        "support.class",
        "entity.name.tag",
        "storage.type.class",
      ],
      settings: { foreground: "#d2a8ff" },
    },
    {
      scope: ["variable", "variable.other", "entity.name.variable"],
      settings: { foreground: "#e6edf3" },
    },
    {
      scope: ["variable.language", "variable.parameter", "meta.parameters"],
      settings: { foreground: "#ffa657" },
    },
    {
      scope: [
        "entity.name.namespace",
        "entity.name.module",
        "meta.import",
        "keyword.import",
        "keyword.export",
      ],
      settings: { foreground: "#d2a8ff" },
    },
    { scope: ["markup.heading"], settings: { foreground: "#79c0ff", fontStyle: "bold" } },
    { scope: ["markup.bold"], settings: { fontStyle: "bold" } },
    { scope: ["markup.italic"], settings: { fontStyle: "italic" } },
    { scope: ["markup.inserted"], settings: { foreground: "#7ee787" } },
    { scope: ["markup.deleted"], settings: { foreground: "#ffa198" } },
    { scope: ["markup.changed"], settings: { foreground: "#ffa657" } },
    { scope: ["invalid", "invalid.illegal"], settings: { foreground: "#ffa198" } },
    { scope: ["punctuation", "meta.brace", "meta.delimiter"], settings: { foreground: "#e6edf3" } },
    {
      scope: ["entity.other.attribute-name", "meta.attribute"],
      settings: { foreground: "#7ee787" },
    },
    {
      scope: ["support.type.property-name", "variable.other.property", "meta.object-literal.key"],
      settings: { foreground: "#79c0ff" },
    },
    {
      scope: ["markup.raw", "markup.inline.raw", "string.regexp"],
      settings: { foreground: "#7ee787" },
    },
    {
      scope: ["markup.link", "markup.underline.link", "string.other.link"],
      settings: { foreground: "#a5d6ff" },
    },
  ],
};

export const THEMES = {
  light: "oxidean-light",
  dark: "oxidean-dark",
} as const;

export type HighlightTheme = (typeof THEMES)[keyof typeof THEMES];

/**
 * Resolve oxidean-light / oxidean-dark for client highlighting.
 * Prefer `html.dark` (set by the FOUC boot script) so we match SSR + first paint
 * instead of re-deriving from localStorage/matchMedia and causing a flicker.
 */
export function clientHighlightTheme(): HighlightTheme {
  if (typeof document !== "undefined") {
    return document.documentElement.classList.contains("dark") ? THEMES.dark : THEMES.light;
  }
  return resolveTheme(readThemePreference()) === "dark" ? THEMES.dark : THEMES.light;
}

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

/**
 * Every Shiki grammar id referenced by the shared language table
 * (`@oxidean/api-client` LANGUAGES — generated from
 * `crates/oxidean-core/src/languages.rs`) plus the in-repo tsrx/ripple
 * grammars. Rows with `shiki: null` render as plaintext and load nothing.
 */
const TABLE_LANGS = [
  ...new Set(
    LANGUAGES.map((l) => l.shiki).filter(
      // tsrx/ripple resolve through the in-repo grammar objects below —
      // passing the bare ids would look them up in Shiki's bundle and fail.
      (s): s is string => !!s && s !== "tsrx" && s !== "ripple",
    ),
  ),
];

let highlighterPromise: Promise<Highlighter> | null = null;

/**
 * Singleton Shiki highlighter with the shared-table langs + in-repo
 * tsrx/ripple grammars (D-19, issue #59). Custom langs are full TextMate
 * grammars — not TypeScript/JavaScript aliases. JS regex engine (forgiving)
 * matches official TSRX demo for large TM grammars.
 */
export async function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [oxideanLightTheme, oxideanDarkTheme],
      langs: [...TABLE_LANGS, "plaintext", tsrxLang, rippleLang],
      engine: createJavaScriptRegexEngine({ forgiving: true }),
    });
  }
  return highlighterPromise;
}

/**
 * Map a repo path to a Shiki language id using the generated shared table —
 * basename match first, then extension; rows without a grammar id fall back
 * to plaintext. Mirrors `language_for_path` in `oxidean_core::languages`.
 */
export function languageIdForPath(filePath: string): string {
  const base = (filePath.split(/[/\\]/).pop() ?? filePath).toLowerCase();
  for (const l of LANGUAGES) {
    if (l.filenames.includes(base)) return l.shiki ?? "plaintext";
  }
  const dot = base.lastIndexOf(".");
  const ext = dot >= 0 ? base.slice(dot + 1) : "";
  if (!ext || ext === base) return "plaintext";
  for (const l of LANGUAGES) {
    if (l.extensions.includes(ext)) return l.shiki ?? "plaintext";
  }
  return "plaintext";
}

export async function highlightCode(
  code: string,
  options: { lang: string; theme?: HighlightTheme },
): Promise<string> {
  const highlighter = await getHighlighter();
  const theme = options.theme ?? THEMES.dark;
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
