import { Check, Copy, FileCode2 } from "@octanejs/lucide";
import { useEffect, useEffectEvent, useState } from "octane";
import type { RepoBlobResponse } from "@oxidean/api-client";
import { MarkdownHtml } from "@/components/repo/markdown-body";
import { buttonVariants } from "@/components/ui/button";
import {
  clientHighlightTheme,
  countCodeLines,
  highlightCode,
  languageIdForPath,
  stripTrailingNewline,
  type HighlightTheme,
} from "@/lib/highlight";
import { isMarkdownPath } from "@/lib/markdown";
import {
  blameHref,
  formatFileSize,
  imageMimeForPath,
  isImagePath,
  parseLineHash,
  rawBlobUrl,
} from "@/lib/repo-browse";
import { parseLfsPointer } from "@/lib/lfs-pointer";
import { apiClient } from "@/lib/api-client";
import { cn } from "@/lib/utils";
import { AppLink } from "@/components/ui/app-link";

type BlobViewerProps = {
  owner: string;
  repo: string;
  blob: RepoBlobResponse;
  /** SSR Shiki HTML when available. */
  highlightedHtml?: string | null;
  /** Theme used for `highlightedHtml` — skip client re-highlight when it matches. */
  highlightTheme?: HighlightTheme;
  /** SSR GFM HTML for markdown paths (Preview tab). */
  markdownHtml?: string | null;
};

function fileBaseName(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] || path || "file";
}

function lineCountLabel(n: number): string {
  return n === 1 ? "1 line" : `${n} lines`;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function plainCodeHtml(text: string): string {
  return `<pre><code>${escapeHtml(stripTrailingNewline(text))}</code></pre>`;
}

/** Syntax-highlighted blob with forge-style header, line numbers, #L permalinks (D-19, D-20). */
export function BlobViewer({
  owner,
  repo,
  blob,
  highlightedHtml = null,
  highlightTheme = null,
  markdownHtml = null,
}: BlobViewerProps) @{
  const text = !blob.is_binary && blob.encoding === "utf-8" && blob.content ? blob.content : "";
  const lfsPointer = text ? parseLfsPointer(text) : null;
  const isMd =
    !!markdownHtml || (!blob.is_binary && blob.encoding === "utf-8" && isMarkdownPath(blob.path));
  const [view, setView] = useState<"preview" | "code">(() => (isMd ? "preview" : "code"));
  const [hl, setHl] = useState<string | null>(
    () => highlightedHtml ?? (text ? plainCodeHtml(text) : null),
  );
  const [range, setRange] = useState<{ start: number; end: number } | null>(null);
  const [copied, setCopied] = useState(false);
  const [lfsDlPending, setLfsDlPending] = useState(false);
  const [lfsDlError, setLfsDlError] = useState("");
  const rawUrl = rawBlobUrl(owner, repo, blob.ref, blob.path);
  const blameUrl = blameHref(owner, repo, blob.ref, blob.path);
  const baseName = fileBaseName(blob.path);
  const lineCount = countCodeLines(text);
  const lineNums = Array.from({ length: lineCount }, (_, i) => i + 1);

  const syncHash = useEffectEvent(() => {
    if (typeof window === "undefined") return;
    setRange(parseLineHash(window.location.hash));
  });

  useEffect(() => {
    syncHash();
    window.addEventListener("hashchange", syncHash);
    return () => window.removeEventListener("hashchange", syncHash);
  }, []);

  useEffect(() => {
    if (blob.is_binary || blob.truncated || blob.encoding !== "utf-8" || !blob.content) {
      setHl(null);
      return;
    }

    let cancelled = false;
    let appliedTheme: HighlightTheme | null =
      highlightedHtml && highlightTheme && clientHighlightTheme() === highlightTheme
        ? highlightTheme
        : null;

    if (appliedTheme && highlightedHtml) {
      setHl((prev) => (prev === highlightedHtml ? prev : highlightedHtml));
    }

    async function run() {
      const theme = clientHighlightTheme();
      if (appliedTheme === theme) return;
      const out = await highlightCode(blob.content ?? "", {
        lang: languageIdForPath(blob.path),
        theme,
      });
      if (cancelled) return;
      appliedTheme = theme;
      setHl((prev) => (prev === out ? prev : out));
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
  }, [
    blob.content,
    blob.path,
    blob.is_binary,
    blob.truncated,
    blob.encoding,
    highlightedHtml,
    highlightTheme,
  ]);

  useEffect(() => {
    if (!range || typeof document === "undefined") return;
    const el = document.getElementById(`L${range.start}`);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [range, hl]);

  function goLine(n: number) {
    window.history.replaceState(null, "", `#L${n}`);
    setRange({ start: n, end: n });
  }

  async function copyContents() {
    if (!text || typeof navigator === "undefined" || !navigator.clipboard) {
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  }

  async function downloadLfsObject() {
    if (!lfsPointer) return;
    setLfsDlError("");
    setLfsDlPending(true);
    try {
      const res = await apiClient.repo.lfs.download({
        owner,
        name: repo,
        oid: lfsPointer.oid,
      });
      if (!res.ok) {
        setLfsDlError(res.error.message || "Download failed.");
        setLfsDlPending(false);
        return;
      }
      const bin = Uint8Array.from(atob(res.data.content), (c) => c.charCodeAt(0));
      const blobUrl = URL.createObjectURL(new Blob([bin], { type: "application/octet-stream" }));
      const a = document.createElement("a");
      a.href = blobUrl;
      a.download = fileBaseName(blob.path);
      a.click();
      URL.revokeObjectURL(blobUrl);
      setLfsDlPending(false);
    } catch {
      setLfsDlError("Can't reach Oxidean. Check your connection and try again.");
      setLfsDlPending(false);
    }
  }

  const showImage =
    blob.is_binary && isImagePath(blob.path) && !!blob.content && blob.encoding === "base64";

  const metaBits: string[] = [];
  if (!blob.is_binary && lineCount > 0) {
    metaBits.push(lineCountLabel(lineCount));
  }
  metaBits.push(formatFileSize(blob.size));

  <div className="overflow-hidden rounded-md border border-border bg-card">
    <div className="flex flex-wrap items-center gap-3 border-b border-border bg-muted/35 px-3 py-2">
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <FileCode2 aria-hidden size={16} className="shrink-0 text-muted-foreground" />
        <span
          className="truncate font-mono text-[14px] font-semibold text-foreground"
          title={blob.path as string}
        >
          {baseName as string}
        </span>
        <span className="shrink-0 text-[13px] text-muted-foreground">
          {metaBits.join(" · ") as string}
        </span>
        @if (lfsPointer) {
          <span
            className="inline-flex shrink-0 items-center rounded-md border border-border bg-muted/50 px-2 py-0.5 text-[12px] font-medium text-foreground"
            data-testid="lfs-pointer-badge"
          >
            LFS
          </span>;
        }
      </div>
      <div className="flex flex-wrap items-center gap-1">
        @if (isMd && markdownHtml) {
          <div
            className="mr-1 flex rounded-md border border-border p-0.5"
            role="group"
            aria-label="Markdown view"
          >
            <button
              type="button"
              className={cn(
                "h-7 rounded px-2 text-[12px] font-medium",
                view === "preview"
                  ? "bg-muted text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
              aria-pressed={view === "preview"}
              onClick={() => setView("preview")}
              data-testid="blob-md-preview"
            >
              Preview
            </button>
            <button
              type="button"
              className={cn(
                "h-7 rounded px-2 text-[12px] font-medium",
                view === "code"
                  ? "bg-muted text-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
              aria-pressed={view === "code"}
              onClick={() => setView("code")}
              data-testid="blob-md-code"
            >
              Code
            </button>
          </div>;
        }
        @if (lfsPointer) {
          <button
            type="button"
            className={cn(buttonVariants({ variant: "secondary" }), "h-8 px-2.5 text-[13px]")}
            disabled={lfsDlPending}
            onClick={() => void downloadLfsObject()}
          >
            {(lfsDlPending ? "Downloading…" : "Download via LFS") as string}
          </button>;
        }
        {/* raw-anchor-ok — /api raw blob endpoint, opens in new tab */}
        <a
          href={rawUrl as string}
          className={cn(buttonVariants({ variant: "ghost" }), "h-8 px-2.5 text-[13px]")}
          target="_blank"
          rel="noreferrer"
        >
          Raw
        </a>
        <AppLink
          href={blameUrl as string}
          className={cn(buttonVariants({ variant: "ghost" }), "h-8 px-2.5 text-[13px]")}
        >
          Blame
        </AppLink>
        @if (!blob.is_binary && text) {
          <button
            type="button"
            className={cn(
              buttonVariants({ variant: "ghost" }),
              "h-8 gap-1.5 px-2.5 text-[13px]",
            )}
            onClick={() => void copyContents()}
            aria-label={copied ? "Copied" : "Copy file contents"}
            title={copied ? "Copied" : "Copy file contents"}
          >
            @if (copied) {
              <Check aria-hidden size={14} />;
            } @else {
              <Copy aria-hidden size={14} />;
            }
            {(copied ? "Copied" : "Copy") as string}
          </button>;
        }
        @if (blob.truncated || blob.is_binary) {
          {
            /* raw-anchor-ok — /api raw blob download */
          }
          <a
            href={rawUrl as string}
            className={cn(buttonVariants({ variant: "secondary" }), "h-8 px-2.5 text-[13px]")}
            download
          >
            Download
          </a>;
        }
      </div>
    </div>

    @if (lfsDlError) {
      <div role="alert" className="border-b border-border px-4 py-2">
        <p className="text-[14px] text-destructive">{lfsDlError as string}</p>
      </div>;
    }

    @if (blob.truncated) {
      <div role="status" className="border-b border-border px-4 py-3">
        <p className="text-[14px] text-muted-foreground">
          This file is too large to display. View raw or download.
        </p>
      </div>;
    }

    @if (showImage) {
      <div className="flex justify-center bg-muted/20 p-6">
        <img
          src={`data:${imageMimeForPath(blob.path)};base64,${blob.content}` as string}
          alt={blob.path as string}
          className="max-h-[70vh] max-w-full"
        />
      </div>;
    } @else {
      @if (blob.is_binary || blob.encoding !== "utf-8") {
        <div className="px-6 py-10 text-center">
          <p className="text-[16px] text-muted-foreground">
            This file can't be previewed. Download it instead.
          </p>
        </div>;
      } @else {
        @if (!blob.truncated) {
          @if (isMd && markdownHtml && view === "preview") {
            <div className="px-6 py-6" data-testid="blob-markdown-preview">
              <MarkdownHtml html={markdownHtml as string} />
            </div>;
          } @else {
            <div className="flex overflow-x-auto">
              <div
                className="sticky left-0 z-10 shrink-0 select-none border-r border-border bg-muted/50 py-3 font-mono text-[12px] leading-5 text-muted-foreground"
                aria-label="Line numbers"
              >
                @for (const n of lineNums; key n) {
                  const wash =
                    range && n >= range.start && n <= range.end
                      ? "bg-primary/15 text-foreground"
                      : "";
                  <button
                    type="button"
                    id={`L${n}` as string}
                    className={
                      `block h-5 min-w-[3.25rem] px-2 text-right leading-5 hover:text-foreground ${wash}` as string
                    }
                    onClick={() => goLine(n)}
                    aria-label={`Line ${n}`}
                  >
                    {String(n) as string}
                  </button>;
                }
              </div>
              <div
                className="blob-code min-w-0 flex-1 overflow-x-auto py-3 font-mono text-[12px] leading-5 [&_pre]:m-0 [&_pre]:bg-transparent! [&_pre]:p-0 [&_pre]:leading-5 [&_code]:block [&_code]:font-mono [&_code]:leading-5 [&_code]:px-3"
                dangerouslySetInnerHTML={{
                  __html: (hl ?? plainCodeHtml(text)) as string,
                }}
              />
            </div>;
          }
        }
      }
    }
  </div>;
}
