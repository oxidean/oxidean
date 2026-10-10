import { useEffect, useState } from "octane";
import { isMarkdownPath, renderGfm } from "@/lib/markdown";
import { cn } from "@/lib/utils";

export { isMarkdownPath };

/** Shared GFM body class — styles live in `styles.css` (`.prose-oxidean`). */
const MARKDOWN_BODY_CLASS = "prose-oxidean max-w-none";

type MarkdownHtmlProps = {
  /** Already sanitized HTML from `renderGfm`. */
  html: string;
  className?: string;
  "data-testid"?: string;
};

/** Presentational wrapper for sanitized GFM HTML. */
export function MarkdownHtml(props: MarkdownHtmlProps) @{
  <div
    className={cn(MARKDOWN_BODY_CLASS, props.className)}
    data-testid={props["data-testid"]}
    dangerouslySetInnerHTML={{ __html: props.html }}
  />;
}

type MarkdownBodyProps = {
  /** Raw markdown source (client-rendered via renderGfm). */
  markdown: string;
  owner?: string;
  repo?: string;
  className?: string;
  /** Shown while async render is in flight (optional). */
  fallback?: string;
  "data-testid"?: string;
};

/**
 * Client-side GFM body for issue/PR/comment/review text.
 * Prefer SSR `renderGfm` + `MarkdownHtml` when the loader already has the source.
 */
export function MarkdownBody(props: MarkdownBodyProps) @{
  const [html, setHtml] = useState(props.fallback ?? "");

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const rendered = await renderGfm(props.markdown || "", {
        owner: props.owner,
        repo: props.repo,
      });
      if (!cancelled) setHtml(rendered);
    })();
    return () => {
      cancelled = true;
    };
  }, [props.markdown, props.owner, props.repo]);

  <MarkdownHtml
    html={html as string}
    className={props.className}
    data-testid={props["data-testid"]}
  />;
}
