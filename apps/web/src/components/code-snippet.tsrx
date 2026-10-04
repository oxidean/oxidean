import { useEffect, useState } from "octane";
import {
  clientHighlightTheme,
  highlightCode,
  languageIdForPath,
  stripTrailingNewline,
  type HighlightTheme,
} from "@/lib/highlight";

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function plainHtml(code: string): string {
  return `<pre><code>${escapeHtml(stripTrailingNewline(code))}</code></pre>`;
}

type CodeSnippetProps = {
  code: string;
  path: string;
  /** Pre-rendered Shiki HTML (SSR) — used as-is while it matches the client theme. */
  highlightedHtml?: string | null;
  /** Theme used for `highlightedHtml` — skip client re-highlight when it matches. */
  highlightTheme?: HighlightTheme | null;
};

/**
 * Short code excerpt (e.g. a search hit line) rendered as escaped text — or
 * pre-rendered Shiki HTML — on SSR and upgraded on the client only when the
 * theme mismatches or flips (same MutationObserver approach as BlobViewer).
 */
export function CodeSnippet({
  code,
  path,
  highlightedHtml = null,
  highlightTheme = null,
}: CodeSnippetProps) @{
  const [html, setHtml] = useState<string>(() => highlightedHtml ?? plainHtml(code));

  useEffect(() => {
    let cancelled = false;
    // Skip the (expensive, all-grammar) client highlighter entirely when the
    // SSR markup already matches the resolved client theme.
    let appliedTheme: HighlightTheme | null =
      highlightedHtml && highlightTheme && clientHighlightTheme() === highlightTheme
        ? highlightTheme
        : null;

    if (appliedTheme && highlightedHtml) {
      setHtml((prev) => (prev === highlightedHtml ? prev : highlightedHtml));
    }

    async function run() {
      const theme = clientHighlightTheme();
      if (appliedTheme === theme) return;
      const out = await highlightCode(code, {
        lang: languageIdForPath(path),
        theme,
      });
      if (cancelled) return;
      appliedTheme = theme;
      setHtml((prev) => (prev === out ? prev : out));
    }

    void run();
    const mo = new MutationObserver(() => {
      void run();
    });
    mo.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => {
      cancelled = true;
      mo.disconnect();
    };
  }, [code, path, highlightedHtml, highlightTheme]);

  <div
    className="mt-1 overflow-x-auto font-mono text-[13px] leading-5 text-foreground [&_code]:font-mono [&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:p-0"
    dangerouslySetInnerHTML={{ __html: html as string }}
  />;
}
