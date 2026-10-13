import { useEffect, useState } from "octane";
import {
  getHighlighter,
  highlightCode,
  highlightCodeSync,
  languageIdForPath,
  stripTrailingNewline,
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
  /** Pre-rendered Shiki HTML (SSR) — theme-complete markup, used as-is. */
  highlightedHtml?: string | null;
};

/**
 * Short code excerpt (e.g. a search hit line) rendered as escaped text — or
 * pre-rendered Shiki HTML — on SSR and upgraded on the client only while the
 * highlighter is still loading. Highlighted markup is dual-theme via
 * `light-dark()`, so theme flips need no re-highlight pass.
 */
export function CodeSnippet({ code, path, highlightedHtml = null }: CodeSnippetProps) @{
  const [html, setHtml] = useState<string>(
    () =>
      highlightedHtml ??
      highlightCodeSync(code, { lang: languageIdForPath(path) }) ??
      plainHtml(code),
  );

  useEffect(() => {
    let cancelled = false;

    if (highlightedHtml) {
      setHtml((prev) => (prev === highlightedHtml ? prev : highlightedHtml));
    } else {
      // Prop change (code/path) — when the highlighter is already resolved,
      // render styled markup synchronously so prop swaps (auth-mode toggles,
      // search-hit pages) never paint a plain-text intermediate frame.
      const sync = highlightCodeSync(code, { lang: languageIdForPath(path) });
      if (sync !== null) {
        setHtml((prev) => (prev === sync ? prev : sync));
      } else {
        // Engine still loading — reflect the new content synchronously so a
        // slow or failed highlight never leaves the previous snippet on
        // screen.
        setHtml((prev) => {
          const next = plainHtml(code);
          return prev === next ? prev : next;
        });
      }
    }

    async function run() {
      // Warm the singleton even when SSR markup is in place — later prop
      // swaps take the synchronous highlightCodeSync path.
      await getHighlighter().catch(() => null);
      if (cancelled || highlightedHtml) return;
      try {
        const out = await highlightCode(code, { lang: languageIdForPath(path) });
        if (cancelled) return;
        setHtml((prev) => (prev === out ? prev : out));
      } catch {
        /* highlighter unavailable — plainHtml fallback already rendered */
      }
    }

    void run();
    return () => {
      cancelled = true;
    };
  }, [code, path, highlightedHtml]);

  <div
    className="mt-1 overflow-x-auto font-mono text-[13px] leading-5 text-foreground [&_code]:font-mono [&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:p-0"
    dangerouslySetInnerHTML={{ __html: html as string }}
  />;
}
