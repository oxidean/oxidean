import { useEffect, useState } from "octane";
import {
  clientHighlightTheme,
  highlightCode,
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

/**
 * Short code excerpt (e.g. a search hit line) rendered as escaped text on SSR
 * and upgraded to Shiki HTML on the client. Re-highlights when the theme flips
 * (same MutationObserver approach as BlobViewer).
 */
export function CodeSnippet({ code, path }: { code: string; path: string }) @{
  const [html, setHtml] = useState<string>(() => plainHtml(code));

  useEffect(() => {
    let cancelled = false;
    async function run() {
      const out = await highlightCode(code, {
        lang: languageIdForPath(path),
        theme: clientHighlightTheme(),
      });
      if (!cancelled) setHtml(out);
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
  }, [code, path]);

  <div
    className="mt-1 overflow-x-auto font-mono text-[13px] leading-5 text-foreground [&_code]:font-mono [&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:p-0"
    dangerouslySetInnerHTML={{ __html: html as string }}
  />;
}
