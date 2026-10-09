import { useEffect, useState } from "octane";
import { MarkdownHtml } from "@/components/repo/markdown-body";
import { Textarea } from "@/components/ui/textarea";
import { renderGfm } from "@/lib/markdown";

type MarkdownWritePreviewProps = {
  id?: string;
  value: string;
  onValueChange: (next: string) => void;
  placeholder?: string;
  rows?: number;
  disabled?: boolean;
  className?: string;
  /** Repo owner for `#N` autolink in Preview (ISS-04). */
  owner?: string;
  /** Repo name for `#N` autolink in Preview (ISS-04). */
  repo?: string;
};

/**
 * Shared Write | Preview markdown control (D-ISS-10).
 * Preview uses renderGfm (sanitize-last); never stores HTML.
 */
export function MarkdownWritePreview(props: MarkdownWritePreviewProps) @{
  const [tab, setTab] = useState<"write" | "preview">("write");
  const [previewHtml, setPreviewHtml] = useState("");

  useEffect(() => {
    if (tab !== "preview") return;
    let cancelled = false;
    void (async () => {
      const html = await renderGfm(props.value || "_Nothing to preview_", {
        owner: props.owner,
        repo: props.repo,
      });
      if (!cancelled) setPreviewHtml(html);
    })();
    return () => {
      cancelled = true;
    };
  }, [tab, props.value, props.owner, props.repo]);

  <div className={(props.className ?? "flex flex-col gap-2") as string}>
    <div className="flex gap-2 border-b border-border pb-0">
      <button
        type="button"
        className={
          (tab === "write"
            ? "border-b-2 border-primary pb-2 text-[14px] font-semibold"
            : "pb-2 text-[14px] text-muted-foreground") as string
        }
        onClick={() => setTab("write")}
      >
        Write
      </button>
      <button
        type="button"
        className={
          (tab === "preview"
            ? "border-b-2 border-primary pb-2 text-[14px] font-semibold"
            : "pb-2 text-[14px] text-muted-foreground") as string
        }
        onClick={() => setTab("preview")}
      >
        Preview
      </button>
    </div>
    @if (tab === "write") {
      <Textarea
        id={props.id}
        rows={props.rows ?? 8}
        value={props.value}
        placeholder={props.placeholder}
        disabled={props.disabled}
        onInput={(e) => props.onValueChange((e.target as HTMLTextAreaElement).value)}
      />;
    } @else {
      <MarkdownHtml
        html={previewHtml as string}
        className="min-h-40 rounded-md border border-border px-4 py-3"
        data-testid="markdown-preview"
      />;
    }
  </div>;
}
