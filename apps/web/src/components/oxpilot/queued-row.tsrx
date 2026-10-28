import type { OxpilotQueuedMessage } from "@oxidean/api-client";

/**
 * One queued-message row — `◐` + explicit "queued"/"still queued" text
 * (the glyph is never the only signal — BR10.3). `role="status"` is
 * static markup here, so a screen reader announces the row once on
 * insert — never re-announces on every parent update.
 */
export function OxpilotQueuedRow(props: {
  queued: OxpilotQueuedMessage;
  stillQueued?: boolean;
  onCancel?: () => void;
}) @{
  <div
    role="status"
    className="flex items-start gap-2 rounded-md border border-dashed border-border bg-muted/30 px-3 py-2"
  >
    <span aria-hidden className="text-[13px] leading-[1.5] text-muted-foreground">
      ◐
    </span>
    <p className="min-w-0 text-[13px] text-foreground">
      <span className="font-medium">
        {(props.stillQueued ? "still queued" : "queued") as string}
      </span>
      {` — ${props.queued.content}` as string}
    </p>
    @if (props.onCancel) {
      <button
        type="button"
        aria-label="Remove queued message"
        className="ml-auto shrink-0 rounded px-1 text-muted-foreground hover:text-foreground"
        onClick={() => props.onCancel?.()}
      >
        ×
      </button>;
    }
  </div>;
}
